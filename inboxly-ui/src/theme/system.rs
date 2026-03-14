//! System colour scheme detection via freedesktop portal D-Bus.
//!
//! Queries `org.freedesktop.portal.Settings` for the system colour scheme
//! preference using `dbus-send` (avoids zbus's Tokio runtime requirement
//! which conflicts with Iced's own async executor).

use tracing::debug;

/// System colour scheme preference from freedesktop portal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemColorScheme {
    /// No preference set (default to light).
    NoPreference,
    /// System prefers dark theme.
    Dark,
    /// System prefers light theme.
    Light,
}

/// Errors from system theme detection.
#[derive(Debug, thiserror::Error)]
pub enum SystemThemeError {
    /// D-Bus query failed.
    #[error("D-Bus query failed: {0}")]
    QueryFailed(String),
}

/// Query the system colour scheme via `dbus-send`.
///
/// Uses `org.freedesktop.portal.Settings.Read` with namespace
/// `org.freedesktop.appearance` and key `color-scheme`.
///
/// The portal returns a `uint32` value:
/// - 0 = no preference
/// - 1 = prefer dark
/// - 2 = prefer light
///
/// # Errors
///
/// Returns `SystemThemeError::QueryFailed` if `dbus-send` is not available
/// or the portal doesn't support the setting.
pub fn query_system_color_scheme() -> Result<SystemColorScheme, SystemThemeError> {
    let output = std::process::Command::new("dbus-send")
        .args([
            "--session",
            "--print-reply=literal",
            "--dest=org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Settings.Read",
            "string:org.freedesktop.appearance",
            "string:color-scheme",
        ])
        .output()
        .map_err(|e| {
            debug!("dbus-send failed to execute: {e}");
            SystemThemeError::QueryFailed(e.to_string())
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        debug!("dbus-send returned error: {stderr}");
        return Err(SystemThemeError::QueryFailed(stderr.into_owned()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Output looks like: "   variant    variant       uint32 1"
    // Extract the last integer on the line.
    let scheme = stdout
        .split_whitespace()
        .rev()
        .find_map(|token| token.parse::<u32>().ok())
        .unwrap_or(0);

    let result = match scheme {
        1 => SystemColorScheme::Dark,
        2 => SystemColorScheme::Light,
        _ => SystemColorScheme::NoPreference,
    };

    debug!("system color scheme from portal: {result:?} (raw value: {scheme})");
    Ok(result)
}
