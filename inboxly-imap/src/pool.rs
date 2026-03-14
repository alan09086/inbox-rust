use std::sync::Arc;
use std::time::Duration;

use async_imap::Session;
use rustls::ClientConfig;
use tokio::net::TcpStream;
use tokio::sync::Semaphore;
use tokio_rustls::client::TlsStream;
use tracing::{info, warn};

use crate::connection;
use crate::error::{ImapError, Result};

/// Configuration for the connection pool.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Maximum concurrent IMAP connections per account.
    pub max_connections: usize,
    /// Timeout for establishing a new connection.
    pub connect_timeout: Duration,
    /// How long an idle connection lives before being closed.
    pub idle_timeout: Duration,
    /// Maximum retry attempts on connection failure.
    pub max_retries: u32,
    /// Base delay for exponential backoff between retries.
    pub retry_base_delay: Duration,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 3,
            connect_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(300),
            max_retries: 3,
            retry_base_delay: Duration::from_secs(1),
        }
    }
}

impl PoolConfig {
    /// Calculate the retry delay for a given attempt using exponential backoff.
    ///
    /// `attempt` is zero-indexed: delay = base * 2^attempt
    pub fn retry_delay(&self, attempt: u32) -> Duration {
        self.retry_base_delay * 2u32.pow(attempt)
    }
}

/// A managed IMAP connection pool for a single account.
///
/// Handles connection acquisition, health checks, and transparent reconnection.
pub struct ConnectionPool {
    host: String,
    port: u16,
    use_starttls: bool,
    tls_config: Arc<ClientConfig>,
    config: PoolConfig,
    semaphore: Arc<Semaphore>,
}

impl ConnectionPool {
    /// Create a new connection pool for the given IMAP server.
    pub fn new(
        host: String,
        port: u16,
        use_starttls: bool,
        tls_config: Arc<ClientConfig>,
        config: PoolConfig,
    ) -> Self {
        let semaphore = Arc::new(Semaphore::new(config.max_connections));
        Self {
            host,
            port,
            use_starttls,
            tls_config,
            config,
            semaphore,
        }
    }

    /// Establish a new unauthenticated IMAP connection.
    ///
    /// Respects the connection semaphore and connect timeout.
    /// Retries with exponential backoff on failure.
    pub async fn connect(&self) -> Result<connection::ImapConnection> {
        let _permit = tokio::time::timeout(
            self.config.connect_timeout,
            self.semaphore.acquire(),
        )
        .await
        .map_err(|_| ImapError::Timeout(self.config.connect_timeout))?
        .map_err(|_| ImapError::PoolExhausted)?;

        // The permit is acquired — we can forget it since async-imap
        // takes ownership of the connection. The pool tracks outstanding
        // connections conceptually; for v1 we use the semaphore to limit
        // concurrency and reconnect on demand.
        // NOTE: In production, we'd hold the permit in a wrapper and
        // return it when the session is returned to the pool.
        // For M6, the semaphore is a concurrency gate only.
        std::mem::forget(_permit);

        let mut last_err = None;

        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                let delay = self.config.retry_delay(attempt - 1);
                warn!(
                    host = %self.host,
                    attempt,
                    delay_ms = delay.as_millis(),
                    "Retrying IMAP connection"
                );
                tokio::time::sleep(delay).await;
            }

            let result = if self.use_starttls {
                connection::connect_starttls(&self.host, self.port, &self.tls_config).await
            } else {
                connection::connect_implicit_tls(&self.host, self.port, &self.tls_config).await
            };

            match result {
                Ok(conn) => {
                    info!(host = %self.host, port = self.port, "Connection established");
                    return Ok(conn);
                }
                Err(e) => {
                    warn!(
                        host = %self.host,
                        attempt,
                        error = %e,
                        "Connection attempt failed"
                    );
                    last_err = Some(e);
                }
            }
        }

        Err(last_err.unwrap_or(ImapError::ConnectionLost {
            reason: "All connection attempts exhausted".to_string(),
        }))
    }

    /// Check if a session is still alive by issuing a NOOP command.
    pub async fn check_health(
        session: &mut Session<TlsStream<TcpStream>>,
    ) -> bool {
        session.noop().await.is_ok()
    }

    /// Returns a reference to the pool configuration.
    pub fn config(&self) -> &PoolConfig {
        &self.config
    }
}

