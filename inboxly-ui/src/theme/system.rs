//! System colour scheme detection via freedesktop portal D-Bus.
//!
//! Queries `org.freedesktop.portal.Settings` for the system colour scheme
//! preference. This is the standard freedesktop way to detect dark mode
//! on both X11 and Wayland.

use tracing::{debug, warn};
use zbus::zvariant::OwnedValue;

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
    /// D-Bus session connection not available.
    #[error("D-Bus session unavailable: {0}")]
    DBusUnavailable(String),

    /// Portal doesn't support the color-scheme setting.
    #[error("portal does not support color-scheme: {0}")]
    PortalUnsupported(String),
}

/// Query the system colour scheme via D-Bus.
///
/// Uses `org.freedesktop.portal.Settings.Read` with namespace
/// `org.freedesktop.appearance` and key `color-scheme`.
///
/// Returns:
/// - `Ok(SystemColorScheme)` on success
/// - `Err` if D-Bus is unavailable or the portal doesn't support the setting
///
/// The portal returns a `u32` value:
/// - 0 = no preference
/// - 1 = prefer dark
/// - 2 = prefer light
///
/// # Errors
///
/// Returns `SystemThemeError::DBusUnavailable` if the D-Bus session bus
/// cannot be reached, or `SystemThemeError::PortalUnsupported` if the
/// portal does not expose the `color-scheme` setting.
pub async fn query_system_color_scheme() -> Result<SystemColorScheme, SystemThemeError> {
    let connection = zbus::Connection::session().await.map_err(|e| {
        debug!("D-Bus session connection failed: {e}");
        SystemThemeError::DBusUnavailable(e.to_string())
    })?;

    let proxy = zbus::Proxy::new(
        &connection,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Settings",
    )
    .await
    .map_err(|e| {
        debug!("failed to build portal proxy: {e}");
        SystemThemeError::DBusUnavailable(e.to_string())
    })?;

    // The Read method returns Variant<Variant<u32>> (double-wrapped).
    let reply: OwnedValue = proxy
        .call("Read", &("org.freedesktop.appearance", "color-scheme"))
        .await
        .map_err(|e: zbus::Error| {
            debug!("portal Settings.Read call failed: {e}");
            SystemThemeError::PortalUnsupported(e.to_string())
        })?;

    // Unwrap the double Variant wrapping to get the u32 value.
    let scheme = unwrap_color_scheme(&reply).unwrap_or_else(|| {
        warn!("could not parse color-scheme value from portal, defaulting to NoPreference");
        0u32
    });

    let result = match scheme {
        1 => SystemColorScheme::Dark,
        2 => SystemColorScheme::Light,
        _ => SystemColorScheme::NoPreference,
    };

    debug!("system color scheme from portal: {result:?} (raw value: {scheme})");
    Ok(result)
}

/// Attempt to unwrap the double-variant color-scheme value to a `u32`.
fn unwrap_color_scheme(value: &OwnedValue) -> Option<u32> {
    use zbus::zvariant::Value;

    // Try direct u32 first.
    if let Ok(v) = <u32>::try_from(value) {
        return Some(v);
    }

    // Try single Variant unwrap: Value::Value(inner) where inner is a u32.
    let val: Value<'_> = value.try_into().ok()?;
    if let Value::Value(inner) = val
        && let Ok(v) = <u32>::try_from(&*inner)
    {
        return Some(v);
    }

    None
}
