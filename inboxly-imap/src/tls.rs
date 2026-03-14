use std::sync::Arc;

use rustls::ClientConfig;
use rustls_pki_types::ServerName;
use tokio::net::TcpStream;
use tokio_rustls::{client::TlsStream, TlsConnector};

use crate::error::{ImapError, Result};

/// Build a rustls `ClientConfig` with Mozilla root certificates.
/// This config is reusable across connections (wrap in `Arc`).
pub fn build_tls_config() -> Arc<ClientConfig> {
    let root_store = rustls::RootCertStore::from_iter(
        webpki_roots::TLS_SERVER_ROOTS.iter().cloned(),
    );

    let config = ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .expect("ring provider supports default TLS versions")
        .with_root_certificates(root_store)
        .with_no_client_auth();

    Arc::new(config)
}

/// Create a `TlsConnector` from a shared config.
pub fn make_connector(config: &Arc<ClientConfig>) -> TlsConnector {
    TlsConnector::from(Arc::clone(config))
}

/// Establish an implicit TLS connection (e.g., port 993).
///
/// Connects via TCP, then immediately wraps in TLS.
pub async fn connect_tls(
    host: &str,
    port: u16,
    tls_config: &Arc<ClientConfig>,
) -> Result<TlsStream<TcpStream>> {
    // Validate the server name before attempting TCP connection.
    let server_name = ServerName::try_from(host.to_owned())
        .map_err(|_| ImapError::InvalidServerName(host.to_owned()))?;

    let addr = format!("{host}:{port}");
    let tcp = TcpStream::connect(&addr).await?;

    let connector = make_connector(tls_config);
    let tls_stream = connector.connect(server_name, tcp).await?;
    Ok(tls_stream)
}

/// Upgrade an existing plain TCP connection to TLS (STARTTLS).
///
/// Called after the IMAP server responds to the STARTTLS command.
pub async fn upgrade_to_tls(
    tcp: TcpStream,
    host: &str,
    tls_config: &Arc<ClientConfig>,
) -> Result<TlsStream<TcpStream>> {
    let server_name = ServerName::try_from(host.to_owned())
        .map_err(|_| ImapError::InvalidServerName(host.to_owned()))?;

    let connector = make_connector(tls_config);
    let tls_stream = connector.connect(server_name, tcp).await?;
    Ok(tls_stream)
}
