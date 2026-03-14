use std::sync::Arc;

use async_imap::Client;
use rustls::ClientConfig;
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tracing::{debug, info};

use crate::error::{ImapError, Result};
use crate::tls;

/// Tracked IMAP capabilities that affect sync strategy.
#[derive(Debug, Clone, Default)]
pub struct ImapCapabilities {
    pub condstore: bool,
    pub idle: bool,
    pub special_use: bool,
    pub xoauth2: bool,
    pub compress_deflate: bool,
    pub move_cmd: bool,
    pub literal_plus: bool,
    pub raw: Vec<String>,
}

/// Parse a list of capability strings into structured flags.
pub fn parse_capabilities(raw: &[String]) -> ImapCapabilities {
    let mut caps = ImapCapabilities {
        raw: raw.to_vec(),
        ..Default::default()
    };

    for cap in raw {
        match cap.to_uppercase().as_str() {
            "CONDSTORE" => caps.condstore = true,
            "IDLE" => caps.idle = true,
            "SPECIAL-USE" => caps.special_use = true,
            s if s.starts_with("AUTH=XOAUTH2") => caps.xoauth2 = true,
            "XOAUTH2" => caps.xoauth2 = true,
            "COMPRESS=DEFLATE" => caps.compress_deflate = true,
            "MOVE" => caps.move_cmd = true,
            "LITERAL+" => caps.literal_plus = true,
            _ => {}
        }
    }

    caps
}

/// An established but unauthenticated IMAP connection.
pub struct ImapConnection {
    pub client: Client<TlsStream<TcpStream>>,
    pub host: String,
    pub port: u16,
    pub tls_config: Arc<ClientConfig>,
}

/// Connect to an IMAP server using implicit TLS (port 993).
pub async fn connect_implicit_tls(
    host: &str,
    port: u16,
    tls_config: &Arc<ClientConfig>,
) -> Result<ImapConnection> {
    info!(host, port, "Connecting to IMAP server (implicit TLS)");

    let tls_stream = tls::connect_tls(host, port, tls_config).await?;
    let client = Client::new(tls_stream);

    debug!(host, "IMAP client created");

    Ok(ImapConnection {
        client,
        host: host.to_owned(),
        port,
        tls_config: Arc::clone(tls_config),
    })
}

/// Connect to an IMAP server using STARTTLS (port 143).
///
/// 1. Opens a plain TCP connection.
/// 2. Reads the server greeting.
/// 3. Issues STARTTLS command.
/// 4. Upgrades to TLS.
/// 5. Returns the TLS-wrapped client.
pub async fn connect_starttls(
    host: &str,
    port: u16,
    tls_config: &Arc<ClientConfig>,
) -> Result<ImapConnection> {
    info!(host, port, "Connecting to IMAP server (STARTTLS)");

    let addr = format!("{host}:{port}");
    let tcp = TcpStream::connect(&addr).await?;

    // Wrap in async-imap client to speak IMAP on the plain connection
    let client = Client::new(tcp);

    // Issue STARTTLS. async-imap's Client doesn't have a direct starttls()
    // method on plain streams, so we send the raw command and then upgrade.
    // The client's into_inner() gives us the underlying stream to upgrade.
    debug!(host, "Sending STARTTLS command");

    // Read the greeting first (async-imap does this on new())
    // Then we need to get the inner stream and upgrade it.
    // async-imap doesn't natively support STARTTLS — we must:
    // 1. Send "STARTTLS" command raw
    // 2. Get the inner stream
    // 3. Upgrade to TLS
    // 4. Re-wrap in a new Client
    //
    // This is a known limitation. If STARTTLS is needed, we handle it
    // at the TCP level before creating the async-imap Client.

    // Alternative approach: connect plain TCP, read greeting manually,
    // send STARTTLS, upgrade, then create Client on the TLS stream.
    drop(client);

    // Re-open and do STARTTLS manually
    let tcp = TcpStream::connect(&addr).await?;

    // Read server greeting
    let mut buf = vec![0u8; 4096];
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut tcp = tcp;
    let n = tcp.read(&mut buf).await?;
    let greeting = String::from_utf8_lossy(&buf[..n]);
    debug!(host, greeting = %greeting, "Server greeting received");

    // Send STARTTLS command
    let tag = "A001";
    let cmd = format!("{tag} STARTTLS\r\n");
    tcp.write_all(cmd.as_bytes()).await?;

    // Read response
    let n = tcp.read(&mut buf).await?;
    let response = String::from_utf8_lossy(&buf[..n]);
    if !response.contains("OK") {
        return Err(ImapError::StarttlsUnsupported);
    }

    // Upgrade to TLS
    let tls_stream = tls::upgrade_to_tls(tcp, host, tls_config).await?;
    let client = Client::new(tls_stream);

    debug!(host, "STARTTLS upgrade complete");

    Ok(ImapConnection {
        client,
        host: host.to_owned(),
        port,
        tls_config: Arc::clone(tls_config),
    })
}

/// Detect capabilities from an authenticated IMAP session.
///
/// Call after login/authenticate — some servers advertise different
/// capabilities post-auth.
pub async fn detect_capabilities(
    session: &mut async_imap::Session<TlsStream<TcpStream>>,
) -> Result<ImapCapabilities> {
    let caps = session.capabilities().await?;
    let raw: Vec<String> = caps
        .iter()
        .map(|c| {
            use async_imap::types::Capability;
            match c {
                Capability::Imap4rev1 => "IMAP4rev1".to_string(),
                Capability::Auth(s) => format!("AUTH={s}"),
                Capability::Atom(s) => s.to_string(),
            }
        })
        .collect();
    debug!(capabilities = ?raw, "Server capabilities detected");
    Ok(parse_capabilities(&raw))
}
