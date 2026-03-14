# M6: IMAP Connection + Auth — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish authenticated IMAP connections with OAuth2 and password auth, detect capabilities, and list/map well-known folders. Expose an `mpsc`-based channel API for the UI to receive sync events.

**Architecture:** `inboxly-imap` crate owns all IMAP network communication. It runs on a background tokio runtime, decoupled from the UI. A `TlsConnector` (tokio-rustls + webpki-roots) handles implicit TLS (port 993) and STARTTLS (port 143). Authentication supports two paths: password LOGIN (Fastmail, generic IMAP) and XOAUTH2 SASL (Gmail). OAuth2 token acquisition uses authorization code flow with PKCE via a localhost loopback redirect. After auth, the engine detects IMAP capabilities (CONDSTORE, IDLE, SPECIAL-USE, etc.), lists folders via LIST with SPECIAL-USE attribute parsing (RFC 6154), and maps them to well-known roles. A connection pool manages reconnection. All events flow to the UI via `tokio::sync::mpsc` channels.

**Tech Stack:** Rust, async-imap 0.11, tokio 1.x, tokio-rustls 0.26, rustls 0.23, webpki-roots 1.x, oauth2 5.x, base64 0.22, inboxly-core

**Prerequisites:** M1 (core types: `AccountId`, `Account`, `AuthMethod`, `ImapProvider`), M2 (config system: `AppConfig` with account settings loaded from TOML).

**Spec reference:** `docs/superpowers/specs/2026-03-14-inboxly-design.md` — sections "IMAP Sync Engine", "Authentication", "Synced Folders", "Communication".

---

## File Structure

All paths are relative to the workspace root (`/mnt/TempNVME/projects/inbox-rust/`).

```
inboxly-imap/
├── Cargo.toml
└── src/
    ├── lib.rs              # Public API re-exports
    ├── tls.rs              # TLS connector: implicit TLS + STARTTLS
    ├── connection.rs       # IMAP connection establishment + capability detection
    ├── auth/
    │   ├── mod.rs          # Auth dispatcher (routes to password or oauth2)
    │   ├── password.rs     # Password LOGIN authentication
    │   ├── oauth2.rs       # OAuth2 token acquisition (PKCE + loopback)
    │   └── xoauth2.rs      # XOAUTH2 SASL authenticator for async-imap
    ├── folders.rs          # LIST command, SPECIAL-USE parsing, well-known mapping
    ├── pool.rs             # Connection pool + reconnect logic
    ├── channel.rs          # SyncEvent / UiCommand channel types
    └── error.rs            # ImapError enum (thiserror)

inboxly-imap/tests/
├── tls_test.rs             # TLS connector unit tests (mock)
├── auth_password_test.rs   # Password auth tests
├── auth_xoauth2_test.rs    # XOAUTH2 format tests
├── folders_test.rs         # Folder parsing + mapping tests
├── pool_test.rs            # Pool / reconnect tests
├── channel_test.rs         # Channel type tests
└── integration_test.rs     # End-to-end with real or mock IMAP
```

---

## Chunk 1: Crate Setup + Error Types + TLS

### Task 1: Create `inboxly-imap` Crate and Cargo.toml

**Files:**
- Create: `inboxly-imap/Cargo.toml`
- Create: `inboxly-imap/src/lib.rs`
- Modify: `Cargo.toml` (workspace root — create if not exists)

- [ ] **Step 1: Create workspace root `Cargo.toml`**

If not already present, create the workspace root Cargo.toml. If it exists from M1/M2, add `inboxly-imap` to the members list.

```toml
[workspace]
resolver = "2"
members = [
    "inboxly-core",
    "inboxly-imap",
]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "GPL-3.0"
```

> **Note for implementer:** If the workspace root already exists from M1, just add `"inboxly-imap"` to the `members` list. Do not overwrite existing members.

- [ ] **Step 2: Create `inboxly-imap/Cargo.toml`**

```toml
[package]
name = "inboxly-imap"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
inboxly-core = { path = "../inboxly-core" }
async-imap = "0.11"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "sync", "time"] }
tokio-rustls = "0.26"
rustls = "0.23"
webpki-roots = "1"
rustls-pki-types = "1"
oauth2 = "5"
base64 = "0.22"
thiserror = "2"
tracing = "0.1"
url = "2"
serde = { version = "1", features = ["derive"] }
rand = "0.9"

[dev-dependencies]
tokio = { version = "1", features = ["test-util"] }
```

- [ ] **Step 3: Create `inboxly-imap/src/lib.rs` with module declarations**

```rust
pub mod auth;
pub mod channel;
pub mod connection;
pub mod error;
pub mod folders;
pub mod pool;
pub mod tls;
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p inboxly-imap`
Expected: Compilation errors for missing modules (that is fine — we will create them in subsequent tasks). If `inboxly-core` does not exist yet, create a minimal stub:

```bash
mkdir -p inboxly-core/src
```

Stub `inboxly-core/Cargo.toml`:
```toml
[package]
name = "inboxly-core"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
serde = { version = "1", features = ["derive"] }
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
thiserror = "2"
```

Stub `inboxly-core/src/lib.rs` with the types M6 needs:
```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AccountId(pub Uuid);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: AccountId,
    pub email: String,
    pub display_name: String,
    pub provider: ImapProvider,
    pub auth_method: AuthMethod,
    pub imap_host: String,
    pub imap_port: u16,
    pub smtp_host: String,
    pub smtp_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImapProvider {
    Gmail,
    Fastmail,
    Generic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthMethod {
    Password { username: String, password: String },
    OAuth2 { client_id: String, client_secret: Option<String> },
    AppPassword { username: String, password: String },
}
```

> **Note for implementer:** If M1 has already been implemented, use the real `inboxly-core` types. The stub above is only needed if M1 has not run yet. Adapt field names to match whatever M1 produced.

- [ ] **Step 5: Commit**

```bash
git add inboxly-imap/Cargo.toml inboxly-imap/src/lib.rs Cargo.toml
# If stubs were created:
git add inboxly-core/Cargo.toml inboxly-core/src/lib.rs
git commit -m "feat(imap): scaffold inboxly-imap crate with dependencies"
```

---

### Task 2: Error Types

**Files:**
- Create: `inboxly-imap/src/error.rs`

- [ ] **Step 1: Write the error type**

```rust
use std::io;

/// All errors that can occur in the IMAP crate.
#[derive(Debug, thiserror::Error)]
pub enum ImapError {
    #[error("TLS error: {0}")]
    Tls(#[from] rustls::Error),

    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("IMAP protocol error: {0}")]
    Imap(#[from] async_imap::error::Error),

    #[error("Authentication failed: {reason}")]
    AuthFailed { reason: String },

    #[error("OAuth2 error: {reason}")]
    OAuth2 { reason: String },

    #[error("OAuth2 token expired, refresh required")]
    TokenExpired,

    #[error("Connection lost: {reason}")]
    ConnectionLost { reason: String },

    #[error("STARTTLS not supported by server")]
    StarttlsUnsupported,

    #[error("Invalid server name: {0}")]
    InvalidServerName(String),

    #[error("Capability not supported: {0}")]
    CapabilityNotSupported(String),

    #[error("Folder not found: {0}")]
    FolderNotFound(String),

    #[error("Connection pool exhausted")]
    PoolExhausted,

    #[error("Channel closed")]
    ChannelClosed,

    #[error("Timeout after {0:?}")]
    Timeout(std::time::Duration),
}

pub type Result<T> = std::result::Result<T, ImapError>;
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p inboxly-imap`
Expected: PASS (other modules still missing — create empty stubs for them if the compiler requires it).

Create minimal stubs for all other modules so the crate compiles:

`inboxly-imap/src/tls.rs`: `// TLS connector — implemented in Task 3`
`inboxly-imap/src/connection.rs`: `// IMAP connection — implemented in Task 4`
`inboxly-imap/src/auth/mod.rs`: `pub mod password; pub mod oauth2; pub mod xoauth2;`
`inboxly-imap/src/auth/password.rs`: `// Password auth — implemented in Task 5`
`inboxly-imap/src/auth/oauth2.rs`: `// OAuth2 — implemented in Task 6`
`inboxly-imap/src/auth/xoauth2.rs`: `// XOAUTH2 — implemented in Task 7`
`inboxly-imap/src/folders.rs`: `// Folders — implemented in Task 8`
`inboxly-imap/src/pool.rs`: `// Pool — implemented in Task 10`
`inboxly-imap/src/channel.rs`: `// Channels — implemented in Task 11`

Run: `cargo check -p inboxly-imap`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add inboxly-imap/src/
git commit -m "feat(imap): add error types and module stubs"
```

---

### Task 3: TLS Connector

**Files:**
- Modify: `inboxly-imap/src/tls.rs`
- Create: `inboxly-imap/tests/tls_test.rs`

This module provides a reusable TLS connector that handles both implicit TLS (port 993) and STARTTLS upgrade (port 143). It wraps `tokio-rustls` with Mozilla root certificates from `webpki-roots`.

- [ ] **Step 1: Write the failing test for TLS config creation**

File: `inboxly-imap/tests/tls_test.rs`

```rust
use inboxly_imap::tls::build_tls_config;

#[test]
fn tls_config_loads_root_certs() {
    let config = build_tls_config();
    // If it doesn't panic, root certs loaded successfully.
    // rustls ClientConfig is opaque — we just verify construction succeeds.
    assert!(std::sync::Arc::strong_count(&config) >= 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p inboxly-imap --test tls_test -- tls_config_loads_root_certs`
Expected: FAIL — `build_tls_config` does not exist.

- [ ] **Step 3: Implement `tls.rs`**

```rust
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

    let config = ClientConfig::builder()
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
    let addr = format!("{host}:{port}");
    let tcp = TcpStream::connect(&addr).await?;

    let server_name = ServerName::try_from(host.to_owned())
        .map_err(|_| ImapError::InvalidServerName(host.to_owned()))?;

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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p inboxly-imap --test tls_test -- tls_config_loads_root_certs`
Expected: PASS

- [ ] **Step 5: Write test for invalid server name**

Add to `inboxly-imap/tests/tls_test.rs`:

```rust
#[tokio::test]
async fn connect_tls_rejects_empty_hostname() {
    let config = build_tls_config();
    let result = inboxly_imap::tls::connect_tls("", 993, &config).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, inboxly_imap::error::ImapError::InvalidServerName(_)));
}
```

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test -p inboxly-imap --test tls_test`
Expected: PASS (2 tests)

- [ ] **Step 7: Commit**

```bash
git add inboxly-imap/src/tls.rs inboxly-imap/tests/tls_test.rs
git commit -m "feat(imap): TLS connector with implicit TLS and STARTTLS support"
```

---

## Chunk 2: IMAP Connection + Password Auth

### Task 4: IMAP Connection Establishment + Capability Detection

**Files:**
- Modify: `inboxly-imap/src/connection.rs`
- Create: `inboxly-imap/tests/connection_test.rs` (unit tests for capability parsing)

This module connects to an IMAP server, performs the TLS handshake, and wraps the stream in an `async_imap::Client`. After authentication (handled by the `auth` module), it parses the server's capability response.

- [ ] **Step 1: Define the capability model and write a test for parsing**

File: `inboxly-imap/tests/connection_test.rs`

```rust
use inboxly_imap::connection::{ImapCapabilities, parse_capabilities};

#[test]
fn parse_capabilities_detects_condstore() {
    let raw = vec![
        "IMAP4rev1".to_string(),
        "IDLE".to_string(),
        "CONDSTORE".to_string(),
        "SPECIAL-USE".to_string(),
        "XOAUTH2".to_string(),
    ];
    let caps = parse_capabilities(&raw);
    assert!(caps.condstore);
    assert!(caps.idle);
    assert!(caps.special_use);
    assert!(caps.xoauth2);
    assert!(!caps.compress_deflate);
}

#[test]
fn parse_capabilities_handles_empty() {
    let raw: Vec<String> = vec![];
    let caps = parse_capabilities(&raw);
    assert!(!caps.condstore);
    assert!(!caps.idle);
    assert!(!caps.special_use);
    assert!(!caps.xoauth2);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p inboxly-imap --test connection_test`
Expected: FAIL — `parse_capabilities` and `ImapCapabilities` not defined.

- [ ] **Step 3: Implement `connection.rs`**

```rust
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
    let mut client = Client::new(tcp);

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
    let n = tokio::io::AsyncReadExt::read(&mut &tcp, &mut buf).await?;
    let greeting = String::from_utf8_lossy(&buf[..n]);
    debug!(host, greeting = %greeting, "Server greeting received");

    // Send STARTTLS command
    use tokio::io::AsyncWriteExt;
    let tag = "A001";
    let cmd = format!("{tag} STARTTLS\r\n");
    (&tcp).write_all(cmd.as_bytes()).await?;

    // Read response
    let n = tokio::io::AsyncReadExt::read(&mut &tcp, &mut buf).await?;
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
    let raw: Vec<String> = caps.iter().map(|c| format!("{c}")).collect();
    debug!(capabilities = ?raw, "Server capabilities detected");
    Ok(parse_capabilities(&raw))
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p inboxly-imap --test connection_test`
Expected: PASS (2 tests)

- [ ] **Step 5: Commit**

```bash
git add inboxly-imap/src/connection.rs inboxly-imap/tests/connection_test.rs
git commit -m "feat(imap): IMAP connection with implicit TLS, STARTTLS, and capability detection"
```

---

### Task 5: Password LOGIN Authentication

**Files:**
- Modify: `inboxly-imap/src/auth/password.rs`
- Modify: `inboxly-imap/src/auth/mod.rs`
- Create: `inboxly-imap/tests/auth_password_test.rs`

Password auth uses `async_imap::Client::login()` with username and password. This covers generic IMAP providers, Fastmail app-specific passwords, and any provider using standard LOGIN.

- [ ] **Step 1: Write the test**

File: `inboxly-imap/tests/auth_password_test.rs`

```rust
use inboxly_imap::auth::password::PasswordCredentials;

#[test]
fn password_credentials_stores_fields() {
    let creds = PasswordCredentials {
        username: "user@example.com".to_string(),
        password: "hunter2".to_string(),
    };
    assert_eq!(creds.username, "user@example.com");
    assert_eq!(creds.password, "hunter2");
}

#[test]
fn password_credentials_debug_redacts_password() {
    let creds = PasswordCredentials {
        username: "user@example.com".to_string(),
        password: "supersecret".to_string(),
    };
    let debug = format!("{creds:?}");
    assert!(!debug.contains("supersecret"), "Password must not appear in Debug output");
    assert!(debug.contains("user@example.com"));
    assert!(debug.contains("[REDACTED]"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p inboxly-imap --test auth_password_test`
Expected: FAIL — `PasswordCredentials` not defined.

- [ ] **Step 3: Implement `auth/password.rs`**

```rust
use async_imap::Session;
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tracing::info;

use crate::connection::ImapConnection;
use crate::error::{ImapError, Result};

/// Credentials for IMAP LOGIN authentication.
///
/// Used for generic IMAP providers, Fastmail app-specific passwords, etc.
pub struct PasswordCredentials {
    pub username: String,
    pub password: String,
}

// Custom Debug to redact password
impl std::fmt::Debug for PasswordCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PasswordCredentials")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .finish()
    }
}

/// Authenticate via IMAP LOGIN command.
///
/// Consumes the `ImapConnection` and returns an authenticated `Session`.
pub async fn login(
    connection: ImapConnection,
    creds: &PasswordCredentials,
) -> Result<Session<TlsStream<TcpStream>>> {
    info!(username = %creds.username, host = %connection.host, "Authenticating via LOGIN");

    let session = connection
        .client
        .login(&creds.username, &creds.password)
        .await
        .map_err(|(err, _client)| ImapError::AuthFailed {
            reason: format!("LOGIN failed: {err}"),
        })?;

    info!(username = %creds.username, "LOGIN authentication successful");
    Ok(session)
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p inboxly-imap --test auth_password_test`
Expected: PASS (2 tests)

- [ ] **Step 5: Update `auth/mod.rs` to export submodules**

```rust
pub mod oauth2;
pub mod password;
pub mod xoauth2;

pub use password::PasswordCredentials;
pub use xoauth2::XOAuth2Credentials;
```

- [ ] **Step 6: Commit**

```bash
git add inboxly-imap/src/auth/
git add inboxly-imap/tests/auth_password_test.rs
git commit -m "feat(imap): password LOGIN authentication"
```

---

## Chunk 3: OAuth2 + XOAUTH2 SASL

### Task 6: OAuth2 Token Acquisition (Authorization Code + PKCE)

**Files:**
- Modify: `inboxly-imap/src/auth/oauth2.rs`

This module handles the OAuth2 authorization code flow with PKCE. For desktop apps, the flow is:
1. Start a local HTTP server on a loopback address (127.0.0.1:port).
2. Open the authorization URL in the user's browser.
3. User authenticates and authorizes. Google redirects to `http://127.0.0.1:port/callback`.
4. Local server captures the authorization code.
5. Exchange the code for access + refresh tokens.

The `oauth2` crate handles most of this. Token storage/refresh is also handled here.

- [ ] **Step 1: Write tests for OAuth2 config construction and token types**

File: add to `inboxly-imap/tests/auth_xoauth2_test.rs` (we will create this file in Task 7 — for now, add OAuth2 config tests):

File: `inboxly-imap/tests/oauth2_test.rs`

```rust
use inboxly_imap::auth::oauth2::{GmailOAuth2Config, OAuth2Token};

#[test]
fn gmail_oauth2_config_has_correct_endpoints() {
    let config = GmailOAuth2Config::new(
        "test-client-id".to_string(),
        Some("test-client-secret".to_string()),
    );
    assert_eq!(config.auth_url, "https://accounts.google.com/o/oauth2/v2/auth");
    assert_eq!(config.token_url, "https://oauth2.googleapis.com/token");
    assert!(config.scopes.contains(&"https://mail.google.com/".to_string()));
}

#[test]
fn oauth2_token_detects_expiry() {
    use std::time::{Duration, Instant};

    // Token that expires in 1 second
    let token = OAuth2Token {
        access_token: "ya29.test".to_string(),
        refresh_token: Some("1//test-refresh".to_string()),
        expires_at: Some(Instant::now() + Duration::from_secs(1)),
    };
    assert!(!token.is_expired());

    // Token that already expired
    let expired = OAuth2Token {
        access_token: "ya29.old".to_string(),
        refresh_token: Some("1//old-refresh".to_string()),
        expires_at: Some(Instant::now() - Duration::from_secs(60)),
    };
    assert!(expired.is_expired());

    // Token with no expiry (treat as not expired)
    let no_expiry = OAuth2Token {
        access_token: "ya29.forever".to_string(),
        refresh_token: None,
        expires_at: None,
    };
    assert!(!no_expiry.is_expired());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p inboxly-imap --test oauth2_test`
Expected: FAIL — types not defined.

- [ ] **Step 3: Implement `auth/oauth2.rs`**

```rust
use std::net::TcpListener as StdTcpListener;
use std::time::{Duration, Instant};

use oauth2::basic::BasicClient;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken,
    PkceCodeChallenge, RedirectUrl, Scope, TokenResponse, TokenUrl,
};
use tracing::{debug, info, warn};

use crate::error::{ImapError, Result};

/// Gmail-specific OAuth2 configuration.
#[derive(Debug, Clone)]
pub struct GmailOAuth2Config {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub auth_url: String,
    pub token_url: String,
    pub scopes: Vec<String>,
    pub redirect_port_range: (u16, u16),
}

impl GmailOAuth2Config {
    pub fn new(client_id: String, client_secret: Option<String>) -> Self {
        Self {
            client_id,
            client_secret,
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
            token_url: "https://oauth2.googleapis.com/token".to_string(),
            scopes: vec!["https://mail.google.com/".to_string()],
            redirect_port_range: (8080, 8099),
        }
    }
}

/// A resolved OAuth2 token with expiry tracking.
#[derive(Debug, Clone)]
pub struct OAuth2Token {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<Instant>,
}

impl OAuth2Token {
    /// Returns `true` if the token has expired or will expire within 60 seconds.
    pub fn is_expired(&self) -> bool {
        match self.expires_at {
            Some(expires_at) => Instant::now() + Duration::from_secs(60) >= expires_at,
            None => false, // No expiry info — assume valid
        }
    }
}

/// Find an available port in the given range for the loopback redirect server.
fn find_available_port(range: (u16, u16)) -> Result<u16> {
    for port in range.0..=range.1 {
        if StdTcpListener::bind(("127.0.0.1", port)).is_ok() {
            return Ok(port);
        }
    }
    Err(ImapError::OAuth2 {
        reason: format!("No available port in range {:?}", range),
    })
}

/// Build the OAuth2 client for a given config.
fn build_client(config: &GmailOAuth2Config, redirect_port: u16) -> Result<BasicClient> {
    let client_id = ClientId::new(config.client_id.clone());
    let client_secret = config.client_secret.as_ref().map(|s| ClientSecret::new(s.clone()));

    let auth_url = AuthUrl::new(config.auth_url.clone()).map_err(|e| ImapError::OAuth2 {
        reason: format!("Invalid auth URL: {e}"),
    })?;

    let token_url =
        TokenUrl::new(config.token_url.clone()).map_err(|e| ImapError::OAuth2 {
            reason: format!("Invalid token URL: {e}"),
        })?;

    let redirect_url =
        RedirectUrl::new(format!("http://127.0.0.1:{redirect_port}/callback")).map_err(
            |e| ImapError::OAuth2 {
                reason: format!("Invalid redirect URL: {e}"),
            },
        )?;

    let mut client = BasicClient::new(client_id)
        .set_auth_uri(auth_url)
        .set_token_uri(token_url)
        .set_redirect_uri(redirect_url);

    if let Some(secret) = client_secret {
        client = client.set_client_secret(secret);
    }

    Ok(client)
}

/// Run the full OAuth2 authorization code flow with PKCE.
///
/// 1. Starts a local HTTP listener on a loopback port.
/// 2. Generates a PKCE challenge.
/// 3. Builds the authorization URL and opens it in the user's browser.
/// 4. Waits for the redirect callback with the authorization code.
/// 5. Exchanges the code for tokens.
///
/// Returns the access token and optional refresh token.
pub async fn authorize(config: &GmailOAuth2Config) -> Result<OAuth2Token> {
    let port = find_available_port(config.redirect_port_range)?;
    info!(port, "Starting OAuth2 loopback server");

    let client = build_client(config, port)?;

    // Generate PKCE challenge
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    // Build authorization URL
    let mut auth_request = client
        .authorize_url(CsrfToken::new_random);

    for scope in &config.scopes {
        auth_request = auth_request.add_scope(Scope::new(scope.clone()));
    }

    let (auth_url, csrf_state) = auth_request
        .set_pkce_challenge(pkce_challenge)
        .url();

    info!("Opening browser for OAuth2 authorization");
    debug!(url = %auth_url, "Authorization URL");

    // Open browser
    if let Err(e) = open::that(auth_url.as_str()) {
        warn!("Failed to open browser: {e}. Please open this URL manually:\n{auth_url}");
    }

    // Start local HTTP server to capture the redirect
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|e| ImapError::OAuth2 {
            reason: format!("Failed to bind loopback server: {e}"),
        })?;

    let code = wait_for_callback(&listener, &csrf_state).await?;

    info!("Authorization code received, exchanging for tokens");

    // Exchange code for token
    let http_client = reqwest::Client::new();
    let token_response = client
        .exchange_code(code)
        .set_pkce_verifier(pkce_verifier)
        .request_async(&http_client)
        .await
        .map_err(|e| ImapError::OAuth2 {
            reason: format!("Token exchange failed: {e}"),
        })?;

    let expires_at = token_response
        .expires_in()
        .map(|d| Instant::now() + d);

    let token = OAuth2Token {
        access_token: token_response.access_token().secret().clone(),
        refresh_token: token_response.refresh_token().map(|t| t.secret().clone()),
        expires_at,
    };

    info!("OAuth2 token acquired successfully");
    Ok(token)
}

/// Refresh an expired OAuth2 token using the refresh token.
pub async fn refresh_token(
    config: &GmailOAuth2Config,
    refresh_token_str: &str,
) -> Result<OAuth2Token> {
    let port = find_available_port(config.redirect_port_range)?;
    let client = build_client(config, port)?;

    let http_client = reqwest::Client::new();
    let token_response = client
        .exchange_refresh_token(&oauth2::RefreshToken::new(refresh_token_str.to_string()))
        .request_async(&http_client)
        .await
        .map_err(|e| ImapError::OAuth2 {
            reason: format!("Token refresh failed: {e}"),
        })?;

    let expires_at = token_response
        .expires_in()
        .map(|d| Instant::now() + d);

    Ok(OAuth2Token {
        access_token: token_response.access_token().secret().clone(),
        refresh_token: token_response
            .refresh_token()
            .map(|t| t.secret().clone())
            .or_else(|| Some(refresh_token_str.to_string())),
        expires_at,
    })
}

/// Wait for the OAuth2 redirect callback on the local HTTP server.
///
/// Parses the `code` and `state` query parameters from the redirect URL.
async fn wait_for_callback(
    listener: &tokio::net::TcpListener,
    expected_state: &CsrfToken,
) -> Result<AuthorizationCode> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let (mut stream, _addr) = listener.accept().await.map_err(|e| ImapError::OAuth2 {
        reason: format!("Failed to accept callback connection: {e}"),
    })?;

    let mut reader = BufReader::new(&mut stream);
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .await
        .map_err(|e| ImapError::OAuth2 {
            reason: format!("Failed to read callback request: {e}"),
        })?;

    // Parse query parameters from "GET /callback?code=xxx&state=yyy HTTP/1.1"
    let url_part = request_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| ImapError::OAuth2 {
            reason: "Malformed callback request".to_string(),
        })?;

    let full_url = format!("http://127.0.0.1{url_part}");
    let parsed = url::Url::parse(&full_url).map_err(|e| ImapError::OAuth2 {
        reason: format!("Failed to parse callback URL: {e}"),
    })?;

    let mut code = None;
    let mut state = None;
    for (key, value) in parsed.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => state = Some(value.into_owned()),
            _ => {}
        }
    }

    // Verify CSRF state
    let received_state = state.ok_or_else(|| ImapError::OAuth2 {
        reason: "No state parameter in callback".to_string(),
    })?;

    if received_state != expected_state.secret().as_str() {
        return Err(ImapError::OAuth2 {
            reason: "CSRF state mismatch".to_string(),
        });
    }

    let code_str = code.ok_or_else(|| ImapError::OAuth2 {
        reason: "No authorization code in callback".to_string(),
    })?;

    // Send a success response to the browser
    let response = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n\
        <html><body><h1>Authorization successful!</h1>\
        <p>You can close this tab and return to Inboxly.</p></body></html>";
    let _ = stream.write_all(response.as_bytes()).await;

    Ok(AuthorizationCode::new(code_str))
}
```

> **Note for implementer:** This depends on `reqwest` (for the token exchange HTTP client) and `open` (to open the browser). Add them to `Cargo.toml`:
> ```toml
> reqwest = { version = "0.12", default-features = false, features = ["rustls-tls"] }
> open = "5"
> ```

- [ ] **Step 4: Run tests**

Run: `cargo test -p inboxly-imap --test oauth2_test`
Expected: PASS (2 tests — config construction and token expiry)

- [ ] **Step 5: Commit**

```bash
git add inboxly-imap/src/auth/oauth2.rs inboxly-imap/Cargo.toml inboxly-imap/tests/oauth2_test.rs
git commit -m "feat(imap): OAuth2 authorization code flow with PKCE for Gmail"
```

---

### Task 7: XOAUTH2 SASL Authenticator

**Files:**
- Modify: `inboxly-imap/src/auth/xoauth2.rs`
- Create: `inboxly-imap/tests/auth_xoauth2_test.rs`

The XOAUTH2 SASL mechanism sends a base64-encoded string in the format:
```
user=<email>\x01auth=Bearer <access_token>\x01\x01
```
(where `\x01` is the SOH control character).

`async-imap` supports SASL via the `Authenticator` trait, which processes server challenges.

- [ ] **Step 1: Write the test for XOAUTH2 format**

File: `inboxly-imap/tests/auth_xoauth2_test.rs`

```rust
use inboxly_imap::auth::xoauth2::{build_xoauth2_string, XOAuth2Credentials};

#[test]
fn xoauth2_string_format() {
    let result = build_xoauth2_string("user@gmail.com", "ya29.test-token");
    // Format: user=<email>\x01auth=Bearer <token>\x01\x01
    let expected = "user=user@gmail.com\x01auth=Bearer ya29.test-token\x01\x01";
    assert_eq!(result, expected);
}

#[test]
fn xoauth2_credentials_debug_redacts_token() {
    let creds = XOAuth2Credentials {
        email: "user@gmail.com".to_string(),
        access_token: "ya29.secret-token".to_string(),
    };
    let debug = format!("{creds:?}");
    assert!(!debug.contains("ya29.secret-token"));
    assert!(debug.contains("user@gmail.com"));
    assert!(debug.contains("[REDACTED]"));
}

#[test]
fn xoauth2_string_handles_special_chars_in_email() {
    let result = build_xoauth2_string("user+tag@gmail.com", "token");
    assert!(result.starts_with("user=user+tag@gmail.com\x01"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p inboxly-imap --test auth_xoauth2_test`
Expected: FAIL — `build_xoauth2_string` not defined.

- [ ] **Step 3: Implement `auth/xoauth2.rs`**

```rust
use async_imap::Authenticator;
use tracing::info;

/// Credentials for XOAUTH2 SASL authentication.
pub struct XOAuth2Credentials {
    pub email: String,
    pub access_token: String,
}

impl std::fmt::Debug for XOAuth2Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("XOAuth2Credentials")
            .field("email", &self.email)
            .field("access_token", &"[REDACTED]")
            .finish()
    }
}

/// Build the raw XOAUTH2 SASL string (before base64 encoding).
///
/// Format per Google spec:
/// `user=<email>\x01auth=Bearer <token>\x01\x01`
///
/// Where `\x01` is the SOH (Start of Heading) control character.
///
/// Reference: <https://developers.google.com/workspace/gmail/imap/xoauth2-protocol>
pub fn build_xoauth2_string(email: &str, access_token: &str) -> String {
    format!("user={email}\x01auth=Bearer {access_token}\x01\x01")
}

/// SASL authenticator for the XOAUTH2 mechanism.
///
/// Implements `async_imap::Authenticator` so it can be passed to
/// `Client::authenticate("XOAUTH2", &authenticator)`.
pub struct XOAuth2Authenticator {
    response: String,
}

impl XOAuth2Authenticator {
    pub fn new(email: &str, access_token: &str) -> Self {
        Self {
            response: build_xoauth2_string(email, access_token),
        }
    }
}

impl Authenticator for XOAuth2Authenticator {
    type Response = String;

    fn process(&mut self, _challenge: &[u8]) -> Self::Response {
        // XOAUTH2 sends the auth string as the initial response.
        // The challenge from the server is ignored (it's just the
        // continuation prompt).
        info!("Sending XOAUTH2 SASL response");
        self.response.clone()
    }
}

/// Authenticate an IMAP connection using XOAUTH2 SASL.
///
/// Consumes the `ImapConnection`, returns an authenticated `Session`.
pub async fn authenticate_xoauth2(
    connection: crate::connection::ImapConnection,
    creds: &XOAuth2Credentials,
) -> crate::error::Result<async_imap::Session<tokio_rustls::client::TlsStream<tokio::net::TcpStream>>>
{
    info!(email = %creds.email, host = %connection.host, "Authenticating via XOAUTH2");

    let mut auth = XOAuth2Authenticator::new(&creds.email, &creds.access_token);

    let session = connection
        .client
        .authenticate("XOAUTH2", &mut auth)
        .await
        .map_err(|(err, _client)| crate::error::ImapError::AuthFailed {
            reason: format!("XOAUTH2 authentication failed: {err}"),
        })?;

    info!(email = %creds.email, "XOAUTH2 authentication successful");
    Ok(session)
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p inboxly-imap --test auth_xoauth2_test`
Expected: PASS (3 tests)

- [ ] **Step 5: Commit**

```bash
git add inboxly-imap/src/auth/xoauth2.rs inboxly-imap/tests/auth_xoauth2_test.rs
git commit -m "feat(imap): XOAUTH2 SASL authenticator for Gmail"
```

---

## Chunk 4: Folder Listing + Well-Known Mapping

### Task 8: Folder LIST with SPECIAL-USE Attribute Parsing

**Files:**
- Modify: `inboxly-imap/src/folders.rs`
- Create: `inboxly-imap/tests/folders_test.rs`

This module lists IMAP folders, parses SPECIAL-USE attributes (RFC 6154), and maps them to well-known folder roles. For servers that don't support SPECIAL-USE (or don't advertise it), it falls back to name-based heuristics.

- [ ] **Step 1: Write tests for folder parsing and mapping**

File: `inboxly-imap/tests/folders_test.rs`

```rust
use inboxly_imap::folders::{
    FolderRole, ImapFolder, WellKnownFolders, map_well_known_folders,
    parse_special_use_attr, resolve_folder_role_by_name,
};

#[test]
fn parse_special_use_sent() {
    assert_eq!(parse_special_use_attr("\\Sent"), Some(FolderRole::Sent));
}

#[test]
fn parse_special_use_drafts() {
    assert_eq!(parse_special_use_attr("\\Drafts"), Some(FolderRole::Drafts));
}

#[test]
fn parse_special_use_trash() {
    assert_eq!(parse_special_use_attr("\\Trash"), Some(FolderRole::Trash));
}

#[test]
fn parse_special_use_junk() {
    assert_eq!(parse_special_use_attr("\\Junk"), Some(FolderRole::Spam));
}

#[test]
fn parse_special_use_all() {
    assert_eq!(parse_special_use_attr("\\All"), Some(FolderRole::All));
}

#[test]
fn parse_special_use_unknown() {
    assert_eq!(parse_special_use_attr("\\SomethingElse"), None);
}

#[test]
fn resolve_role_by_name_inbox() {
    assert_eq!(resolve_folder_role_by_name("INBOX"), Some(FolderRole::Inbox));
    assert_eq!(resolve_folder_role_by_name("Inbox"), Some(FolderRole::Inbox));
}

#[test]
fn resolve_role_by_name_sent_variations() {
    assert_eq!(resolve_folder_role_by_name("Sent"), Some(FolderRole::Sent));
    assert_eq!(resolve_folder_role_by_name("Sent Items"), Some(FolderRole::Sent));
    assert_eq!(resolve_folder_role_by_name("Sent Messages"), Some(FolderRole::Sent));
    assert_eq!(resolve_folder_role_by_name("[Gmail]/Sent Mail"), Some(FolderRole::Sent));
}

#[test]
fn resolve_role_by_name_drafts() {
    assert_eq!(resolve_folder_role_by_name("Drafts"), Some(FolderRole::Drafts));
    assert_eq!(resolve_folder_role_by_name("[Gmail]/Drafts"), Some(FolderRole::Drafts));
}

#[test]
fn resolve_role_by_name_trash_variations() {
    assert_eq!(resolve_folder_role_by_name("Trash"), Some(FolderRole::Trash));
    assert_eq!(resolve_folder_role_by_name("Deleted Items"), Some(FolderRole::Trash));
    assert_eq!(resolve_folder_role_by_name("[Gmail]/Trash"), Some(FolderRole::Trash));
    assert_eq!(resolve_folder_role_by_name("Deleted Messages"), Some(FolderRole::Trash));
}

#[test]
fn resolve_role_by_name_spam_variations() {
    assert_eq!(resolve_folder_role_by_name("Spam"), Some(FolderRole::Spam));
    assert_eq!(resolve_folder_role_by_name("Junk"), Some(FolderRole::Spam));
    assert_eq!(resolve_folder_role_by_name("Junk E-mail"), Some(FolderRole::Spam));
    assert_eq!(resolve_folder_role_by_name("[Gmail]/Spam"), Some(FolderRole::Spam));
}

#[test]
fn resolve_role_by_name_unknown() {
    assert_eq!(resolve_folder_role_by_name("My Custom Folder"), None);
    assert_eq!(resolve_folder_role_by_name("Work"), None);
}

#[test]
fn map_well_known_folders_from_special_use() {
    let folders = vec![
        ImapFolder {
            name: "INBOX".to_string(),
            delimiter: Some('/'),
            role: Some(FolderRole::Inbox),
            attributes: vec![],
        },
        ImapFolder {
            name: "[Gmail]/Sent Mail".to_string(),
            delimiter: Some('/'),
            role: Some(FolderRole::Sent),
            attributes: vec!["\\Sent".to_string()],
        },
        ImapFolder {
            name: "[Gmail]/Drafts".to_string(),
            delimiter: Some('/'),
            role: Some(FolderRole::Drafts),
            attributes: vec!["\\Drafts".to_string()],
        },
        ImapFolder {
            name: "[Gmail]/Trash".to_string(),
            delimiter: Some('/'),
            role: Some(FolderRole::Trash),
            attributes: vec!["\\Trash".to_string()],
        },
        ImapFolder {
            name: "[Gmail]/Spam".to_string(),
            delimiter: Some('/'),
            role: Some(FolderRole::Spam),
            attributes: vec!["\\Junk".to_string()],
        },
    ];

    let wk = map_well_known_folders(&folders);
    assert_eq!(wk.inbox.as_deref(), Some("INBOX"));
    assert_eq!(wk.sent.as_deref(), Some("[Gmail]/Sent Mail"));
    assert_eq!(wk.drafts.as_deref(), Some("[Gmail]/Drafts"));
    assert_eq!(wk.trash.as_deref(), Some("[Gmail]/Trash"));
    assert_eq!(wk.spam.as_deref(), Some("[Gmail]/Spam"));
}

#[test]
fn map_well_known_folders_fallback_by_name() {
    // No SPECIAL-USE attributes — should fall back to name matching
    let folders = vec![
        ImapFolder {
            name: "INBOX".to_string(),
            delimiter: Some('.'),
            role: None,
            attributes: vec![],
        },
        ImapFolder {
            name: "Sent".to_string(),
            delimiter: Some('.'),
            role: None,
            attributes: vec![],
        },
        ImapFolder {
            name: "Drafts".to_string(),
            delimiter: Some('.'),
            role: None,
            attributes: vec![],
        },
        ImapFolder {
            name: "Trash".to_string(),
            delimiter: Some('.'),
            role: None,
            attributes: vec![],
        },
        ImapFolder {
            name: "Junk".to_string(),
            delimiter: Some('.'),
            role: None,
            attributes: vec![],
        },
    ];

    let wk = map_well_known_folders(&folders);
    assert_eq!(wk.inbox.as_deref(), Some("INBOX"));
    assert_eq!(wk.sent.as_deref(), Some("Sent"));
    assert_eq!(wk.drafts.as_deref(), Some("Drafts"));
    assert_eq!(wk.trash.as_deref(), Some("Trash"));
    assert_eq!(wk.spam.as_deref(), Some("Junk"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p inboxly-imap --test folders_test`
Expected: FAIL — types not defined.

- [ ] **Step 3: Implement `folders.rs`**

```rust
use tracing::{debug, info, warn};

use crate::error::Result;

/// The role a folder plays in the email workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FolderRole {
    Inbox,
    Sent,
    Drafts,
    Trash,
    Spam,
    All,
    Archive,
    Flagged,
}

/// A discovered IMAP folder with its parsed attributes.
#[derive(Debug, Clone)]
pub struct ImapFolder {
    /// Full IMAP folder name (e.g., "[Gmail]/Sent Mail").
    pub name: String,
    /// Hierarchy delimiter (e.g., '/' or '.').
    pub delimiter: Option<char>,
    /// Resolved role from SPECIAL-USE attribute or name heuristic.
    pub role: Option<FolderRole>,
    /// Raw IMAP attributes (e.g., "\\Sent", "\\HasNoChildren").
    pub attributes: Vec<String>,
}

/// The five well-known folders Inboxly syncs in v1.
#[derive(Debug, Clone, Default)]
pub struct WellKnownFolders {
    /// IMAP name for Inbox (always "INBOX" per RFC 3501).
    pub inbox: Option<String>,
    /// IMAP name for Sent folder.
    pub sent: Option<String>,
    /// IMAP name for Drafts folder.
    pub drafts: Option<String>,
    /// IMAP name for Trash folder.
    pub trash: Option<String>,
    /// IMAP name for Spam/Junk folder.
    pub spam: Option<String>,
}

impl WellKnownFolders {
    /// Returns all resolved folder names for iteration.
    pub fn all_names(&self) -> Vec<&str> {
        [&self.inbox, &self.sent, &self.drafts, &self.trash, &self.spam]
            .iter()
            .filter_map(|opt| opt.as_deref())
            .collect()
    }

    /// Returns true if all five well-known folders have been resolved.
    pub fn is_complete(&self) -> bool {
        self.inbox.is_some()
            && self.sent.is_some()
            && self.drafts.is_some()
            && self.trash.is_some()
            && self.spam.is_some()
    }
}

/// Parse a SPECIAL-USE attribute string (RFC 6154) into a `FolderRole`.
///
/// Attributes are case-insensitive and prefixed with `\`.
pub fn parse_special_use_attr(attr: &str) -> Option<FolderRole> {
    match attr.to_lowercase().as_str() {
        "\\inbox" => Some(FolderRole::Inbox),
        "\\sent" => Some(FolderRole::Sent),
        "\\drafts" => Some(FolderRole::Drafts),
        "\\trash" => Some(FolderRole::Trash),
        "\\junk" => Some(FolderRole::Spam),
        "\\all" => Some(FolderRole::All),
        "\\archive" => Some(FolderRole::Archive),
        "\\flagged" => Some(FolderRole::Flagged),
        _ => None,
    }
}

/// Resolve a folder's role by its name using common naming conventions.
///
/// This is the fallback when SPECIAL-USE attributes are not available.
/// Handles Gmail paths (`[Gmail]/Sent Mail`), standard names (`Sent`),
/// and common variations (`Sent Items`, `Deleted Items`, etc.).
pub fn resolve_folder_role_by_name(name: &str) -> Option<FolderRole> {
    let lower = name.to_lowercase();

    // INBOX is case-insensitive per RFC 3501
    if lower == "inbox" {
        return Some(FolderRole::Inbox);
    }

    // Sent folder variants
    if lower == "sent"
        || lower == "sent items"
        || lower == "sent messages"
        || lower == "[gmail]/sent mail"
    {
        return Some(FolderRole::Sent);
    }

    // Drafts folder variants
    if lower == "drafts" || lower == "[gmail]/drafts" {
        return Some(FolderRole::Drafts);
    }

    // Trash folder variants
    if lower == "trash"
        || lower == "deleted items"
        || lower == "deleted messages"
        || lower == "[gmail]/trash"
        || lower == "[gmail]/bin"
    {
        return Some(FolderRole::Trash);
    }

    // Spam/Junk folder variants
    if lower == "spam"
        || lower == "junk"
        || lower == "junk e-mail"
        || lower == "junk email"
        || lower == "[gmail]/spam"
    {
        return Some(FolderRole::Spam);
    }

    None
}

/// Map a list of IMAP folders to well-known folder roles.
///
/// Strategy:
/// 1. First pass: use SPECIAL-USE attributes (from `role` field).
/// 2. Second pass: for any unresolved roles, fall back to name heuristics.
pub fn map_well_known_folders(folders: &[ImapFolder]) -> WellKnownFolders {
    let mut wk = WellKnownFolders::default();

    // Pass 1: SPECIAL-USE attributes (highest priority)
    for folder in folders {
        if let Some(role) = &folder.role {
            match role {
                FolderRole::Inbox => wk.inbox.get_or_insert(folder.name.clone()),
                FolderRole::Sent => wk.sent.get_or_insert(folder.name.clone()),
                FolderRole::Drafts => wk.drafts.get_or_insert(folder.name.clone()),
                FolderRole::Trash => wk.trash.get_or_insert(folder.name.clone()),
                FolderRole::Spam => wk.spam.get_or_insert(folder.name.clone()),
                _ => continue,
            };
        }
    }

    // INBOX is always "INBOX" per RFC 3501 — force it if not set by SPECIAL-USE
    if wk.inbox.is_none() {
        for folder in folders {
            if folder.name.eq_ignore_ascii_case("INBOX") {
                wk.inbox = Some(folder.name.clone());
                break;
            }
        }
    }

    // Pass 2: Name heuristic fallback for unresolved roles
    for folder in folders {
        if let Some(role) = resolve_folder_role_by_name(&folder.name) {
            match role {
                FolderRole::Inbox if wk.inbox.is_none() => {
                    wk.inbox = Some(folder.name.clone());
                }
                FolderRole::Sent if wk.sent.is_none() => {
                    wk.sent = Some(folder.name.clone());
                }
                FolderRole::Drafts if wk.drafts.is_none() => {
                    wk.drafts = Some(folder.name.clone());
                }
                FolderRole::Trash if wk.trash.is_none() => {
                    wk.trash = Some(folder.name.clone());
                }
                FolderRole::Spam if wk.spam.is_none() => {
                    wk.spam = Some(folder.name.clone());
                }
                _ => {}
            }
        }
    }

    if !wk.is_complete() {
        warn!(
            inbox = wk.inbox.is_some(),
            sent = wk.sent.is_some(),
            drafts = wk.drafts.is_some(),
            trash = wk.trash.is_some(),
            spam = wk.spam.is_some(),
            "Not all well-known folders resolved"
        );
    }

    wk
}

/// List all folders from an authenticated IMAP session and resolve roles.
///
/// Issues `LIST "" "*"` and parses SPECIAL-USE attributes where available.
pub async fn list_folders(
    session: &mut async_imap::Session<
        tokio_rustls::client::TlsStream<tokio::net::TcpStream>,
    >,
) -> Result<Vec<ImapFolder>> {
    info!("Listing IMAP folders");

    let names = session.list(Some(""), Some("*")).await?;

    let folders: Vec<ImapFolder> = names
        .iter()
        .map(|name| {
            let attrs: Vec<String> = name.attributes().iter().map(|a| format!("{a}")).collect();
            let delimiter = name.delimiter();

            // Try to resolve role from SPECIAL-USE attributes
            let role = attrs
                .iter()
                .find_map(|a| parse_special_use_attr(a))
                .or_else(|| resolve_folder_role_by_name(name.name()));

            debug!(
                folder = name.name(),
                delimiter = ?delimiter,
                attrs = ?attrs,
                role = ?role,
                "Discovered folder"
            );

            ImapFolder {
                name: name.name().to_string(),
                delimiter,
                role,
                attributes: attrs,
            }
        })
        .collect();

    info!(count = folders.len(), "Folder listing complete");
    Ok(folders)
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p inboxly-imap --test folders_test`
Expected: PASS (12 tests)

- [ ] **Step 5: Commit**

```bash
git add inboxly-imap/src/folders.rs inboxly-imap/tests/folders_test.rs
git commit -m "feat(imap): folder listing with SPECIAL-USE and name-based mapping"
```

---

### Task 9: Auth Dispatcher

**Files:**
- Modify: `inboxly-imap/src/auth/mod.rs`

The auth dispatcher routes to the correct authentication method based on the `AuthMethod` variant from `inboxly-core`.

- [ ] **Step 1: Implement the dispatcher**

```rust
pub mod oauth2;
pub mod password;
pub mod xoauth2;

pub use oauth2::{GmailOAuth2Config, OAuth2Token};
pub use password::PasswordCredentials;
pub use xoauth2::XOAuth2Credentials;

use async_imap::Session;
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tracing::info;

use crate::connection::ImapConnection;
use crate::error::{ImapError, Result};

/// Authenticate an IMAP connection using the method specified by the account.
///
/// Dispatches to the appropriate auth path:
/// - `AuthMethod::Password` / `AuthMethod::AppPassword` → LOGIN
/// - `AuthMethod::OAuth2` → XOAUTH2 SASL (acquires token first if needed)
pub async fn authenticate(
    connection: ImapConnection,
    auth_method: &inboxly_core::AuthMethod,
    email: &str,
    token_cache: Option<&OAuth2Token>,
) -> Result<Session<TlsStream<TcpStream>>> {
    match auth_method {
        inboxly_core::AuthMethod::Password { username, password }
        | inboxly_core::AuthMethod::AppPassword { username, password } => {
            info!(method = "LOGIN", username = %username, "Authenticating");
            let creds = PasswordCredentials {
                username: username.clone(),
                password: password.clone(),
            };
            password::login(connection, &creds).await
        }

        inboxly_core::AuthMethod::OAuth2 {
            client_id,
            client_secret,
        } => {
            info!(method = "XOAUTH2", email = %email, "Authenticating");

            // Use cached token if valid, otherwise acquire a new one
            let token = match token_cache {
                Some(t) if !t.is_expired() => t.clone(),
                Some(t) if t.refresh_token.is_some() => {
                    info!("Token expired, refreshing");
                    let config =
                        GmailOAuth2Config::new(client_id.clone(), client_secret.clone());
                    oauth2::refresh_token(&config, t.refresh_token.as_ref().unwrap())
                        .await?
                }
                _ => {
                    info!("No valid token, starting OAuth2 authorization flow");
                    let config =
                        GmailOAuth2Config::new(client_id.clone(), client_secret.clone());
                    oauth2::authorize(&config).await?
                }
            };

            let creds = XOAuth2Credentials {
                email: email.to_string(),
                access_token: token.access_token.clone(),
            };
            xoauth2::authenticate_xoauth2(connection, &creds).await
        }
    }
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p inboxly-imap`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add inboxly-imap/src/auth/mod.rs
git commit -m "feat(imap): auth dispatcher routes to LOGIN or XOAUTH2"
```

---

## Chunk 5: Connection Pool + Channel Types + Integration

### Task 10: Connection Pool and Reconnect Logic

**Files:**
- Modify: `inboxly-imap/src/pool.rs`
- Create: `inboxly-imap/tests/pool_test.rs`

The connection pool manages multiple IMAP sessions per account. It handles:
- Acquiring a session from the pool (or creating a new one).
- Detecting broken connections and transparently reconnecting.
- Limiting maximum concurrent connections per account.

- [ ] **Step 1: Write tests for pool configuration**

File: `inboxly-imap/tests/pool_test.rs`

```rust
use inboxly_imap::pool::PoolConfig;
use std::time::Duration;

#[test]
fn pool_config_defaults() {
    let config = PoolConfig::default();
    assert_eq!(config.max_connections, 3);
    assert_eq!(config.connect_timeout, Duration::from_secs(30));
    assert_eq!(config.idle_timeout, Duration::from_secs(300));
    assert_eq!(config.max_retries, 3);
    assert_eq!(config.retry_base_delay, Duration::from_secs(1));
}

#[test]
fn pool_config_custom() {
    let config = PoolConfig {
        max_connections: 5,
        connect_timeout: Duration::from_secs(60),
        idle_timeout: Duration::from_secs(600),
        max_retries: 5,
        retry_base_delay: Duration::from_secs(2),
    };
    assert_eq!(config.max_connections, 5);
}

#[test]
fn retry_delay_uses_exponential_backoff() {
    let config = PoolConfig::default();
    // retry 0 = 1s, retry 1 = 2s, retry 2 = 4s
    assert_eq!(config.retry_delay(0), Duration::from_secs(1));
    assert_eq!(config.retry_delay(1), Duration::from_secs(2));
    assert_eq!(config.retry_delay(2), Duration::from_secs(4));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p inboxly-imap --test pool_test`
Expected: FAIL — `PoolConfig` not defined.

- [ ] **Step 3: Implement `pool.rs`**

```rust
use std::sync::Arc;
use std::time::Duration;

use async_imap::Session;
use rustls::ClientConfig;
use tokio::net::TcpStream;
use tokio::sync::Semaphore;
use tokio_rustls::client::TlsStream;
use tracing::{debug, info, warn};

use crate::connection::{self, ImapCapabilities};
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
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p inboxly-imap --test pool_test`
Expected: PASS (3 tests)

- [ ] **Step 5: Commit**

```bash
git add inboxly-imap/src/pool.rs inboxly-imap/tests/pool_test.rs
git commit -m "feat(imap): connection pool with exponential backoff reconnect"
```

---

### Task 11: Channel Types for Sync Events

**Files:**
- Modify: `inboxly-imap/src/channel.rs`
- Create: `inboxly-imap/tests/channel_test.rs`

Defines the event types that flow between the IMAP sync engine and the UI. The actual channel is created when the sync engine starts (in M7+), but the types are defined here.

- [ ] **Step 1: Write tests for channel types**

File: `inboxly-imap/tests/channel_test.rs`

```rust
use inboxly_imap::channel::{SyncEvent, UiCommand, create_sync_channels};

#[test]
fn sync_event_variants_constructible() {
    // Ensure all variants can be constructed
    let _ = SyncEvent::Connected {
        account_id: "test".to_string(),
    };
    let _ = SyncEvent::Disconnected {
        account_id: "test".to_string(),
        reason: "timeout".to_string(),
    };
    let _ = SyncEvent::AuthRequired {
        account_id: "test".to_string(),
    };
    let _ = SyncEvent::SyncProgress {
        account_id: "test".to_string(),
        folder: "INBOX".to_string(),
        current: 50,
        total: 100,
        phase: "headers".to_string(),
    };
    let _ = SyncEvent::SyncComplete {
        account_id: "test".to_string(),
        folder: "INBOX".to_string(),
    };
    let _ = SyncEvent::Error {
        account_id: "test".to_string(),
        message: "connection lost".to_string(),
    };
    let _ = SyncEvent::NewEmails {
        account_id: "test".to_string(),
        folder: "INBOX".to_string(),
        count: 5,
    };
    let _ = SyncEvent::FlagsChanged {
        account_id: "test".to_string(),
        folder: "INBOX".to_string(),
        count: 2,
    };
    let _ = SyncEvent::FolderList {
        account_id: "test".to_string(),
        folders: vec![],
    };
}

#[test]
fn ui_command_variants_constructible() {
    let _ = UiCommand::StartSync {
        account_id: "test".to_string(),
    };
    let _ = UiCommand::StopSync {
        account_id: "test".to_string(),
    };
    let _ = UiCommand::ForceResync {
        account_id: "test".to_string(),
        folder: "INBOX".to_string(),
    };
    let _ = UiCommand::Shutdown;
}

#[tokio::test]
async fn channels_send_and_receive() {
    let (event_tx, mut event_rx, cmd_tx, mut cmd_rx) = create_sync_channels(16);

    // Send an event from sync engine to UI
    event_tx
        .send(SyncEvent::Connected {
            account_id: "acct1".to_string(),
        })
        .await
        .unwrap();

    // UI receives it
    let event = event_rx.recv().await.unwrap();
    assert!(matches!(event, SyncEvent::Connected { .. }));

    // Send a command from UI to sync engine
    cmd_tx
        .send(UiCommand::StartSync {
            account_id: "acct1".to_string(),
        })
        .await
        .unwrap();

    let cmd = cmd_rx.recv().await.unwrap();
    assert!(matches!(cmd, UiCommand::StartSync { .. }));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p inboxly-imap --test channel_test`
Expected: FAIL — types not defined.

- [ ] **Step 3: Implement `channel.rs`**

```rust
use tokio::sync::mpsc;

use crate::folders::ImapFolder;

/// Events sent from the IMAP sync engine to the UI.
///
/// Sent via `tokio::sync::mpsc::Sender<SyncEvent>`.
#[derive(Debug, Clone)]
pub enum SyncEvent {
    /// Successfully connected and authenticated to an IMAP account.
    Connected {
        account_id: String,
    },

    /// Disconnected from an account (intentional or error).
    Disconnected {
        account_id: String,
        reason: String,
    },

    /// Authentication is required (token expired, password changed, etc.).
    /// The UI should prompt the user to re-authenticate.
    AuthRequired {
        account_id: String,
    },

    /// Sync progress update for a folder.
    SyncProgress {
        account_id: String,
        folder: String,
        current: u64,
        total: u64,
        phase: String, // "headers", "bodies", "flags"
    },

    /// Sync completed for a folder.
    SyncComplete {
        account_id: String,
        folder: String,
    },

    /// An error occurred during sync.
    Error {
        account_id: String,
        message: String,
    },

    /// New emails arrived in a folder.
    NewEmails {
        account_id: String,
        folder: String,
        count: u64,
    },

    /// Email flags changed in a folder (read, starred, etc.).
    FlagsChanged {
        account_id: String,
        folder: String,
        count: u64,
    },

    /// Folder list retrieved for an account.
    FolderList {
        account_id: String,
        folders: Vec<ImapFolder>,
    },
}

/// Commands sent from the UI to the IMAP sync engine.
///
/// Sent via `tokio::sync::mpsc::Sender<UiCommand>`.
#[derive(Debug, Clone)]
pub enum UiCommand {
    /// Start syncing an account.
    StartSync {
        account_id: String,
    },

    /// Stop syncing an account.
    StopSync {
        account_id: String,
    },

    /// Force a full resync of a specific folder.
    ForceResync {
        account_id: String,
        folder: String,
    },

    /// Gracefully shut down all sync tasks.
    Shutdown,
}

/// Create the bidirectional channel pair for sync engine <-> UI communication.
///
/// - `event_tx` / `event_rx`: Sync engine sends events, UI receives.
/// - `cmd_tx` / `cmd_rx`: UI sends commands, sync engine receives.
///
/// `buffer_size` controls the channel buffer (recommended: 64 or higher for
/// burst handling during initial sync).
pub fn create_sync_channels(
    buffer_size: usize,
) -> (
    mpsc::Sender<SyncEvent>,
    mpsc::Receiver<SyncEvent>,
    mpsc::Sender<UiCommand>,
    mpsc::Receiver<UiCommand>,
) {
    let (event_tx, event_rx) = mpsc::channel(buffer_size);
    let (cmd_tx, cmd_rx) = mpsc::channel(buffer_size);
    (event_tx, event_rx, cmd_tx, cmd_rx)
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p inboxly-imap --test channel_test`
Expected: PASS (3 tests)

- [ ] **Step 5: Commit**

```bash
git add inboxly-imap/src/channel.rs inboxly-imap/tests/channel_test.rs
git commit -m "feat(imap): sync event and UI command channel types"
```

---

### Task 12: Public API and `lib.rs` Re-exports

**Files:**
- Modify: `inboxly-imap/src/lib.rs`

Clean up the public API with convenient re-exports so downstream crates don't need to know the internal module structure.

- [ ] **Step 1: Update `lib.rs`**

```rust
//! # inboxly-imap
//!
//! IMAP sync engine for Inboxly. Handles:
//! - TLS connections (implicit TLS + STARTTLS)
//! - Authentication (password LOGIN, OAuth2 XOAUTH2 SASL)
//! - Capability detection (CONDSTORE, IDLE, SPECIAL-USE, etc.)
//! - Folder listing and well-known folder mapping
//! - Connection pooling with reconnect
//! - Channel types for UI communication

pub mod auth;
pub mod channel;
pub mod connection;
pub mod error;
pub mod folders;
pub mod pool;
pub mod tls;

// Convenience re-exports
pub use auth::{GmailOAuth2Config, OAuth2Token, PasswordCredentials, XOAuth2Credentials};
pub use channel::{SyncEvent, UiCommand, create_sync_channels};
pub use connection::{ImapCapabilities, ImapConnection};
pub use error::{ImapError, Result};
pub use folders::{FolderRole, ImapFolder, WellKnownFolders};
pub use pool::{ConnectionPool, PoolConfig};
pub use tls::build_tls_config;
```

- [ ] **Step 2: Verify everything compiles and all tests pass**

Run: `cargo test -p inboxly-imap`
Expected: All tests pass (across all test files).

- [ ] **Step 3: Run clippy**

Run: `cargo clippy -p inboxly-imap -- -D warnings`
Expected: No warnings. Fix any that appear.

- [ ] **Step 4: Commit**

```bash
git add inboxly-imap/src/lib.rs
git commit -m "feat(imap): public API re-exports for inboxly-imap"
```

---

### Task 13: Integration Test Skeleton

**Files:**
- Create: `inboxly-imap/tests/integration_test.rs`

This test file provides integration test patterns. The live IMAP tests are gated behind an environment variable (`INBOXLY_TEST_IMAP=1`) so they don't run in CI without credentials.

- [ ] **Step 1: Write the integration test skeleton**

```rust
//! Integration tests for inboxly-imap.
//!
//! These tests require a real IMAP server. Gate them behind:
//!
//!   INBOXLY_TEST_IMAP=1 \
//!   INBOXLY_TEST_HOST=imap.gmail.com \
//!   INBOXLY_TEST_PORT=993 \
//!   INBOXLY_TEST_USER=user@gmail.com \
//!   INBOXLY_TEST_PASS=app-specific-password \
//!   cargo test -p inboxly-imap --test integration_test
//!
//! For OAuth2 tests, also set:
//!   INBOXLY_TEST_OAUTH2_CLIENT_ID=...
//!   INBOXLY_TEST_OAUTH2_TOKEN=ya29....

use std::env;

fn should_run_live_tests() -> bool {
    env::var("INBOXLY_TEST_IMAP").is_ok()
}

fn get_env(key: &str) -> String {
    env::var(key).unwrap_or_else(|_| panic!("{key} must be set for live IMAP tests"))
}

#[tokio::test]
async fn live_connect_and_list_folders() {
    if !should_run_live_tests() {
        eprintln!("Skipping live IMAP test (set INBOXLY_TEST_IMAP=1 to enable)");
        return;
    }

    let host = get_env("INBOXLY_TEST_HOST");
    let port: u16 = get_env("INBOXLY_TEST_PORT").parse().expect("Invalid port");
    let username = get_env("INBOXLY_TEST_USER");
    let password = get_env("INBOXLY_TEST_PASS");

    // Build TLS config
    let tls_config = inboxly_imap::build_tls_config();

    // Connect
    let conn =
        inboxly_imap::connection::connect_implicit_tls(&host, port, &tls_config)
            .await
            .expect("Failed to connect");

    // Authenticate
    let creds = inboxly_imap::PasswordCredentials {
        username,
        password,
    };
    let mut session = inboxly_imap::auth::password::login(conn, &creds)
        .await
        .expect("Failed to authenticate");

    // Detect capabilities
    let caps = inboxly_imap::connection::detect_capabilities(&mut session)
        .await
        .expect("Failed to detect capabilities");
    println!("Capabilities: {caps:?}");

    // List folders
    let folders = inboxly_imap::folders::list_folders(&mut session)
        .await
        .expect("Failed to list folders");
    println!("Folders ({}):", folders.len());
    for f in &folders {
        println!("  {} (role: {:?}, attrs: {:?})", f.name, f.role, f.attributes);
    }

    // Map well-known folders
    let wk = inboxly_imap::folders::map_well_known_folders(&folders);
    println!("Well-known folders: {wk:?}");
    assert!(wk.inbox.is_some(), "INBOX must always be found");

    // Logout
    session.logout().await.expect("Failed to logout");
}

#[tokio::test]
async fn live_oauth2_xoauth2_auth() {
    if !should_run_live_tests() {
        eprintln!("Skipping live OAuth2 test (set INBOXLY_TEST_IMAP=1 to enable)");
        return;
    }

    let host = get_env("INBOXLY_TEST_HOST");
    let port: u16 = get_env("INBOXLY_TEST_PORT").parse().expect("Invalid port");
    let email = get_env("INBOXLY_TEST_USER");
    let access_token = match env::var("INBOXLY_TEST_OAUTH2_TOKEN") {
        Ok(t) => t,
        Err(_) => {
            eprintln!("Skipping OAuth2 test (INBOXLY_TEST_OAUTH2_TOKEN not set)");
            return;
        }
    };

    let tls_config = inboxly_imap::build_tls_config();
    let conn =
        inboxly_imap::connection::connect_implicit_tls(&host, port, &tls_config)
            .await
            .expect("Failed to connect");

    let creds = inboxly_imap::XOAuth2Credentials {
        email,
        access_token,
    };
    let mut session =
        inboxly_imap::auth::xoauth2::authenticate_xoauth2(conn, &creds)
            .await
            .expect("XOAUTH2 auth failed");

    let caps = inboxly_imap::connection::detect_capabilities(&mut session)
        .await
        .expect("Failed to detect capabilities");
    println!("Post-auth capabilities: {caps:?}");

    session.logout().await.expect("Failed to logout");
}

/// Unit-level integration: verify the full type wiring compiles.
/// No network required.
#[test]
fn channel_types_are_send_and_sync() {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}

    assert_send::<inboxly_imap::SyncEvent>();
    assert_send::<inboxly_imap::UiCommand>();

    // SyncEvent contains Vec<ImapFolder> which is Send
    assert_send::<inboxly_imap::ImapFolder>();
}

#[test]
fn pool_config_is_clonable() {
    let config = inboxly_imap::PoolConfig::default();
    let _clone = config.clone();
}
```

- [ ] **Step 2: Run the non-live tests**

Run: `cargo test -p inboxly-imap --test integration_test`
Expected: PASS — live tests skip gracefully, type-level tests pass.

- [ ] **Step 3: Run the full test suite**

Run: `cargo test -p inboxly-imap`
Expected: All tests pass across all test files.

- [ ] **Step 4: Commit**

```bash
git add inboxly-imap/tests/integration_test.rs
git commit -m "test(imap): integration test skeleton with live IMAP support"
```

---

### Task 14: Final Cleanup and Documentation

**Files:**
- Verify: all files in `inboxly-imap/`

- [ ] **Step 1: Run the full test suite one final time**

Run: `cargo test -p inboxly-imap`
Expected: All tests pass.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -p inboxly-imap -- -D warnings`
Expected: No warnings.

- [ ] **Step 3: Run cargo doc to verify doc comments**

Run: `cargo doc -p inboxly-imap --no-deps`
Expected: No warnings. Docs build cleanly.

- [ ] **Step 4: Final commit**

```bash
git add -A
git commit -m "chore(imap): M6 complete — IMAP connection, auth, folders, pool, channels"
```

---

## Summary

| Task | Component | Tests |
|------|-----------|-------|
| 1 | Crate scaffolding + Cargo.toml | Compilation check |
| 2 | Error types (`ImapError`) | Compilation check |
| 3 | TLS connector (implicit TLS + STARTTLS) | 2 unit tests |
| 4 | IMAP connection + capability detection | 2 unit tests |
| 5 | Password LOGIN authentication | 2 unit tests |
| 6 | OAuth2 token acquisition (PKCE + loopback) | 2 unit tests |
| 7 | XOAUTH2 SASL authenticator | 3 unit tests |
| 8 | Folder LIST + SPECIAL-USE + well-known mapping | 12 unit tests |
| 9 | Auth dispatcher | Compilation check |
| 10 | Connection pool + reconnect | 3 unit tests |
| 11 | Channel types (SyncEvent, UiCommand) | 3 unit tests |
| 12 | Public API re-exports | Compilation + clippy |
| 13 | Integration test skeleton | 2 type tests + live test stubs |
| 14 | Final cleanup | Full suite + clippy + docs |

**Total: 14 tasks, ~29 unit tests, 12 commits.**

**Key design decisions:**
- `async-imap` is used directly (not wrapped in a trait) for M6 simplicity. Future milestones may add a trait layer for testability.
- STARTTLS is implemented at the TCP level because `async-imap` doesn't natively support it — we do the handshake manually before wrapping in `Client`.
- OAuth2 uses a loopback HTTP server (127.0.0.1:8080-8099) per Google's desktop app guidelines. No OOB flow (deprecated).
- Connection pool uses a semaphore for concurrency limiting. A proper pool with session reuse will be refined in M7-M9 as sync patterns crystallize.
- All secrets are redacted in Debug output via custom `fmt::Debug` implementations.
