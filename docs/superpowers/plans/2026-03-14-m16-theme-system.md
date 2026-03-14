# M16: Theme System — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan.

**Goal:** Implement light and dark themes with BigTop design tokens and system theme detection.

**Architecture:** `InboxlyTheme` struct holds all colour/dimension/typography tokens. Two constructors (`light()`, `dark()`) provide the BigTop values from the spec. System theme detection queries `org.freedesktop.portal.Settings` via `zbus` D-Bus. Manual override persisted in SQLite settings table (M3). `InboxlyTheme` converts to Iced's `Theme::Custom(Arc<Custom>)` for widget styling. Constants (bundle colours, avatar colours, dimensions, typography) are theme-independent and live in dedicated modules.

**Tech Stack:** Rust, iced (Theme/Palette/Custom), zbus (D-Bus), inboxly-core (ThemePreference from M2), inboxly-store (settings table from M3)

**Prerequisites:**
- M15 complete — Iced shell running, `inboxly-ui` crate exists with `Cargo.toml`, `src/lib.rs`, basic `InboxlyApp` struct, toolbar, nav drawer layout
- M3 complete — SQLite settings table with key/value text pairs, `SettingsStore` API for get/set

---

## Task 1: Add theme dependencies to inboxly-ui

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/Cargo.toml`

Add `zbus` for D-Bus system theme detection. The `iced` dependency should already exist from M15. Ensure `inboxly-core` and `inboxly-store` are listed as workspace dependencies.

Add or verify these dependencies:

```toml
[dependencies]
iced = { version = "0.13", features = ["tokio"] }
zbus = { version = "5", default-features = false, features = ["tokio"] }
tokio = { version = "1", features = ["rt"] }
tracing = "0.1"

inboxly-core = { path = "../inboxly-core" }
inboxly-store = { path = "../inboxly-store" }
```

**Note:** `zbus` 5.x uses `tokio` by default. The `default-features = false` + `features = ["tokio"]` ensures we get the tokio backend without pulling in async-std.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add zbus dependency for system theme detection`

---

## Task 2: Define colour token struct — `ThemeColors`

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme/colors.rs` (new file)

Create the `ThemeColors` struct containing all colour tokens from the spec's Theme System section. These are the values that differ between light and dark modes.

```rust
use iced::Color;

/// All colour tokens that vary between light and dark themes.
///
/// Values come from Google Inbox's BigTop APK design tokens.
/// See spec: Theme System > Light Theme / Dark Theme tables.
#[derive(Debug, Clone, Copy)]
pub struct ThemeColors {
    /// Main window background (`#ececec` light, `#121212` dark).
    pub background: Color,
    /// Card/surface background (`#ffffff` light, `#1e1e1e` dark).
    pub surface: Color,
    /// Selected item background (`#ebf2ff` light, `#1a2744` dark).
    pub surface_selected: Color,
    /// Primary text colour (`#212121` light, `#e0e0e0` dark).
    pub text_primary: Color,
    /// Secondary/muted text colour (`#757575` light, `#9e9e9e` dark).
    pub text_secondary: Color,
    /// Divider/stroke colour (`#e0e0e0` light, `#2c2c2c` dark).
    pub divider: Color,
    /// Toolbar colour for Inbox view (`#4285f4` light, `#1a3a6e` dark).
    pub toolbar_inbox: Color,
    /// Toolbar colour for Done view (`#0f9d58` light, `#0b5e35` dark).
    pub toolbar_done: Color,
    /// Toolbar colour for Snoozed view (`#ef6c00` light, `#8f4100` dark).
    pub toolbar_snoozed: Color,
    /// Toolbar text/icon colour (white on coloured toolbars).
    pub toolbar_text: Color,
    /// Whether this is a dark theme.
    pub is_dark: bool,
}

/// Parse a hex colour string like `#4285f4` into an `iced::Color`.
///
/// Panics if the string is not a valid 6-digit hex colour (const-safe in practice
/// since we only call this with string literals).
const fn hex(hex: &str) -> Color {
    let bytes = hex.as_bytes();
    let offset = if bytes[0] == b'#' { 1 } else { 0 };

    let r = hex_byte(bytes[offset], bytes[offset + 1]);
    let g = hex_byte(bytes[offset + 2], bytes[offset + 3]);
    let b = hex_byte(bytes[offset + 4], bytes[offset + 5]);

    Color {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    }
}

const fn hex_byte(hi: u8, lo: u8) -> u8 {
    hex_digit(hi) * 16 + hex_digit(lo)
}

const fn hex_digit(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => panic!("invalid hex digit"),
    }
}
```

**Note:** The `hex()` helper is `const fn` so all colour values can be compile-time constants. Iced's `Color` fields are `f32` (0.0-1.0), not u8.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add ThemeColors struct and const hex colour parser`

---

## Task 3: Light theme constructor

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme/colors.rs` (append)

Add the `light()` constructor with exact BigTop values from the spec.

```rust
impl ThemeColors {
    /// BigTop light theme — the Google Inbox baseline.
    ///
    /// Spec reference: Theme System > Light Theme table.
    pub const fn light() -> Self {
        Self {
            background: hex("#ececec"),
            surface: hex("#ffffff"),
            surface_selected: hex("#ebf2ff"),
            text_primary: hex("#212121"),
            text_secondary: hex("#757575"),
            divider: hex("#e0e0e0"),
            toolbar_inbox: hex("#4285f4"),
            toolbar_done: hex("#0f9d58"),
            toolbar_snoozed: hex("#ef6c00"),
            toolbar_text: hex("#ffffff"),
            is_dark: false,
        }
    }
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement light theme with BigTop colour tokens`

---

## Task 4: Dark theme constructor

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme/colors.rs` (append to `impl ThemeColors`)

Add the `dark()` constructor with spec dark values.

```rust
    /// Dark theme — desaturated variants of the BigTop palette.
    ///
    /// Spec reference: Theme System > Dark Theme table.
    pub const fn dark() -> Self {
        Self {
            background: hex("#121212"),
            surface: hex("#1e1e1e"),
            surface_selected: hex("#1a2744"),
            text_primary: hex("#e0e0e0"),
            text_secondary: hex("#9e9e9e"),
            divider: hex("#2c2c2c"),
            toolbar_inbox: hex("#1a3a6e"),
            toolbar_done: hex("#0b5e35"),
            toolbar_snoozed: hex("#8f4100"),
            toolbar_text: hex("#ffffff"),
            is_dark: true,
        }
    }
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement dark theme colour tokens`

---

## Task 5: Bundle category colours — theme-independent constants

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme/bundle_colors.rs` (new file)

These colours are constant across light/dark themes per the spec.

```rust
use iced::Color;
use super::colors::hex;

/// Bundle category title and badge colours.
///
/// These are constant across light and dark themes.
/// Spec reference: Theme System > Bundle Category Colours table.
pub struct BundleCategoryColor {
    /// Title text colour for the bundle name.
    pub title: Color,
    /// Pastel background for the unread badge.
    pub badge: Color,
}

/// Social bundle colours: red title, pink badge.
pub const SOCIAL: BundleCategoryColor = BundleCategoryColor {
    title: hex("#d23f31"),
    badge: hex("#faebea"),
};

/// Promos bundle colours: cyan title, light cyan badge.
pub const PROMOS: BundleCategoryColor = BundleCategoryColor {
    title: hex("#00acc1"),
    badge: hex("#e5f6f9"),
};

/// Updates bundle colours: deep orange title, light orange badge.
pub const UPDATES: BundleCategoryColor = BundleCategoryColor {
    title: hex("#f4511e"),
    badge: hex("#feede8"),
};

/// Finance bundle colours: green title, light green badge.
pub const FINANCE: BundleCategoryColor = BundleCategoryColor {
    title: hex("#558b2f"),
    badge: hex("#eef3ea"),
};

/// Purchases bundle colours: brown title, light brown badge.
pub const PURCHASES: BundleCategoryColor = BundleCategoryColor {
    title: hex("#6d4c41"),
    badge: hex("#f0edec"),
};

/// Travel bundle colours: purple title, light purple badge.
pub const TRAVEL: BundleCategoryColor = BundleCategoryColor {
    title: hex("#8e24aa"),
    badge: hex("#f3e9f6"),
};

/// Forums bundle colours: indigo title, light indigo badge.
pub const FORUMS: BundleCategoryColor = BundleCategoryColor {
    title: hex("#3949ab"),
    badge: hex("#ebecf6"),
};

/// Low Priority bundle colours: dark grey title, light grey badge.
pub const LOW_PRIORITY: BundleCategoryColor = BundleCategoryColor {
    title: hex("#212121"),
    badge: hex("#e5e5e5"),
};

/// Look up bundle category colours by `BundleCategory`.
///
/// Returns `LOW_PRIORITY` for `Custom` categories (user can override later).
pub fn for_category(category: &inboxly_core::BundleCategory) -> &'static BundleCategoryColor {
    use inboxly_core::BundleCategory;
    match category {
        BundleCategory::Social => &SOCIAL,
        BundleCategory::Promos => &PROMOS,
        BundleCategory::Updates => &UPDATES,
        BundleCategory::Finance => &FINANCE,
        BundleCategory::Purchases => &PURCHASES,
        BundleCategory::Travel => &TRAVEL,
        BundleCategory::Forums => &FORUMS,
        BundleCategory::LowPriority => &LOW_PRIORITY,
        BundleCategory::Saved => &LOW_PRIORITY,
        BundleCategory::Custom(_) => &LOW_PRIORITY,
    }
}
```

**Note:** The `hex()` function must be made `pub(crate)` in `colors.rs` (or `pub(super)` — adjust visibility in Task 2's `hex()` definition). The `for_category` function references `inboxly_core::BundleCategory` which was defined in M1.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add bundle category colour constants`

---

## Task 6: Avatar letter tile colours — 26-colour A-Z palette

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme/avatar_colors.rs` (new file)

All 26 letter colours plus a default, constant across themes.

```rust
use iced::Color;
use super::colors::hex;

/// Avatar letter tile colours for A-Z.
///
/// These are constant across light and dark themes.
/// Spec reference: Theme System > Avatar Letter Tile Colours.
pub const AVATAR_COLORS: [Color; 27] = [
    hex("#e06055"), // A
    hex("#ed6192"), // B
    hex("#ba68c8"), // C
    hex("#9575cd"), // D
    hex("#7986cb"), // E
    hex("#5e97f6"), // F
    hex("#4fc3f7"), // G
    hex("#58d0e1"), // H
    hex("#4fb6ac"), // I
    hex("#57bb8a"), // J
    hex("#9ccc65"), // K
    hex("#d4e157"), // L
    hex("#fdd835"), // M
    hex("#f6bf32"), // N
    hex("#f5a631"), // O
    hex("#f18864"), // P
    hex("#c2c2c2"), // Q
    hex("#90a4ae"), // R
    hex("#a1887f"), // S
    hex("#a3a3a3"), // T
    hex("#afb6e0"), // U
    hex("#b39ddb"), // V
    hex("#c2c2c2"), // W
    hex("#80deea"), // X
    hex("#bcaaa4"), // Y
    hex("#aed581"), // Z
    hex("#efefef"), // default (index 26)
];

/// Get the avatar colour for a given character.
///
/// Maps A-Z (case-insensitive) to the corresponding colour.
/// Returns the default colour for non-letter characters.
pub const fn for_letter(c: char) -> Color {
    let index = match c {
        'A' | 'a' => 0,
        'B' | 'b' => 1,
        'C' | 'c' => 2,
        'D' | 'd' => 3,
        'E' | 'e' => 4,
        'F' | 'f' => 5,
        'G' | 'g' => 6,
        'H' | 'h' => 7,
        'I' | 'i' => 8,
        'J' | 'j' => 9,
        'K' | 'k' => 10,
        'L' | 'l' => 11,
        'M' | 'm' => 12,
        'N' | 'n' => 13,
        'O' | 'o' => 14,
        'P' | 'p' => 15,
        'Q' | 'q' => 16,
        'R' | 'r' => 17,
        'S' | 's' => 18,
        'T' | 't' => 19,
        'U' | 'u' => 20,
        'V' | 'v' => 21,
        'W' | 'w' => 22,
        'X' | 'x' => 23,
        'Y' | 'y' => 24,
        'Z' | 'z' => 25,
        _ => 26, // default
    };
    AVATAR_COLORS[index]
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add avatar letter tile colour palette (A-Z)`

---

## Task 7: Dimension constants

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme/dimensions.rs` (new file)

All dimensions from the spec's Dimensions table. Values are in logical pixels (dp from the APK maps 1:1 to Iced logical pixels). These are constant across themes.

```rust
/// Layout dimension constants from the BigTop APK.
///
/// All values are in logical pixels (1dp = 1 logical pixel at 1x scaling).
/// On HiDPI displays, Iced's built-in scaling applies automatically.
///
/// Spec reference: Theme System > Dimensions table.

// -- Toolbar --
/// Toolbar height in logical pixels.
pub const TOOLBAR_HEIGHT: f32 = 56.0;
/// Toolbar elevation (shadow depth).
pub const TOOLBAR_ELEVATION: f32 = 2.0;

// -- Navigation Drawer --
/// Nav drawer width when open.
pub const NAV_DRAWER_WIDTH: f32 = 264.0;
/// Nav drawer item height.
pub const NAV_DRAWER_ITEM_HEIGHT: f32 = 48.0;

// -- Spacing --
/// Default margin and padding.
pub const DEFAULT_PADDING: f32 = 16.0;

// -- Avatars --
/// Avatar circle diameter.
pub const AVATAR_DIAMETER: f32 = 40.0;
/// Avatar column width (avatar + surrounding space in list rows).
pub const AVATAR_COLUMN_WIDTH: f32 = 72.0;

// -- List Items --
/// Card elevation for list item cards.
pub const LIST_ITEM_ELEVATION: f32 = 2.0;
/// Corner radius for list item cards (flat in BigTop).
pub const LIST_ITEM_CORNER_RADIUS: f32 = 0.0;

// -- Section Headers --
/// Section header height (Pinned, Today, This Month, etc.).
pub const SECTION_HEADER_HEIGHT: f32 = 48.0;

// -- FAB (Floating Action Button) --
/// Main FAB diameter.
pub const FAB_DIAMETER: f32 = 56.0;
/// Mini FAB diameter (speed dial children).
pub const MINI_FAB_DIAMETER: f32 = 40.0;
/// FAB margin from screen edges.
pub const FAB_EDGE_MARGIN: f32 = 13.0;

// -- Snooze Picker --
/// Snooze picker grid total width.
pub const SNOOZE_GRID_WIDTH: f32 = 288.0;
/// Snooze option cell width.
pub const SNOOZE_CELL_WIDTH: f32 = 142.0;
/// Snooze option cell height.
pub const SNOOZE_CELL_HEIGHT: f32 = 122.0;

// -- Compose --
/// Maximum width of the compose view.
pub const COMPOSE_MAX_WIDTH: f32 = 920.0;

// -- Dividers --
/// Divider line thickness (1 physical pixel; Iced handles DPI).
pub const DIVIDER_THICKNESS: f32 = 1.0;
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add BigTop dimension constants`

---

## Task 8: Typography constants

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme/typography.rs` (new file)

Typography sizes and weights from the spec. `sp` from the APK maps 1:1 to logical pixels on desktop.

```rust
use iced::font::Weight;

/// Typography constants from the BigTop APK.
///
/// Sizes are in logical pixels (sp from APK = 1:1 on desktop).
/// Accessibility font scaling is handled by the window manager on desktop,
/// not by the app (Android's sp scaling is not applicable here).
///
/// Spec reference: Theme System > Typography table.

/// Toolbar title text.
pub const TOOLBAR_TITLE_SIZE: f32 = 20.0;
pub const TOOLBAR_TITLE_WEIGHT: Weight = Weight::Normal;

/// Email title / sender name in list view.
pub const EMAIL_TITLE_SIZE: f32 = 16.0;
pub const EMAIL_TITLE_WEIGHT: Weight = Weight::Normal;
/// Bold variant for unread emails.
pub const EMAIL_TITLE_WEIGHT_UNREAD: Weight = Weight::Bold;

/// Author name in conversation view.
pub const AUTHOR_NAME_SIZE: f32 = 14.0;
pub const AUTHOR_NAME_WEIGHT: Weight = Weight::Normal;

/// Snippet/preview text in list view.
pub const SNIPPET_SIZE: f32 = 14.0;
pub const SNIPPET_WEIGHT: Weight = Weight::Normal;

/// Timestamp text.
pub const TIMESTAMP_SIZE: f32 = 12.0;
pub const TIMESTAMP_WEIGHT: Weight = Weight::Normal;

/// Section header text (Today, This Month, etc.).
pub const SECTION_HEADER_SIZE: f32 = 14.0;
pub const SECTION_HEADER_WEIGHT: Weight = Weight::Bold;

/// Unread count badge text.
pub const BADGE_SIZE: f32 = 16.0;
pub const BADGE_WEIGHT: Weight = Weight::Bold;

/// Nav drawer item text.
pub const NAV_ITEM_SIZE: f32 = 14.0;
pub const NAV_ITEM_WEIGHT: Weight = Weight::Medium;

/// Compose view subject line.
pub const COMPOSE_SUBJECT_SIZE: f32 = 18.0;
pub const COMPOSE_SUBJECT_WEIGHT: Weight = Weight::Bold;

/// Compose view body text.
pub const COMPOSE_BODY_SIZE: f32 = 16.0;
pub const COMPOSE_BODY_WEIGHT: Weight = Weight::Normal;
```

**Note:** Iced uses `iced::font::Weight` for font weights. If the Iced version in use has `Weight` at a different path (e.g., `iced::Font` with weight field), adjust accordingly. The implementer should check `cargo doc -p iced --open` and find the correct path.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add BigTop typography constants`

---

## Task 9: Theme module root — wire up submodules

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme/mod.rs` (new file)

Create the `theme` module that re-exports all submodules.

```rust
//! Inboxly theme system — BigTop design tokens.
//!
//! # Modules
//!
//! - [`colors`] — Light/dark colour tokens (`ThemeColors`)
//! - [`bundle_colors`] — Bundle category colours (constant across themes)
//! - [`avatar_colors`] — Avatar letter tile A-Z palette (constant across themes)
//! - [`dimensions`] — Layout dimension constants from BigTop APK
//! - [`typography`] — Font size and weight constants from BigTop APK

pub mod avatar_colors;
pub mod bundle_colors;
pub mod colors;
pub mod dimensions;
pub mod typography;

pub use colors::ThemeColors;
```

**Also:** In `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/lib.rs`, add:

```rust
pub mod theme;
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): create theme module with submodule re-exports`

---

## Task 10: `InboxlyTheme` — main theme struct with Iced integration

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme/mod.rs` (append)

Define `InboxlyTheme` — the app-level theme type that wraps `ThemeColors` and converts to Iced's `Theme`.

```rust
use iced::theme::{self, Palette};
use iced::Theme;
use std::sync::Arc;

/// The main theme struct for the Inboxly application.
///
/// Wraps `ThemeColors` (the BigTop colour tokens) and provides conversion
/// to Iced's `Theme` type for widget styling.
#[derive(Debug, Clone)]
pub struct InboxlyTheme {
    /// The colour tokens for this theme variant.
    pub colors: ThemeColors,
    /// Cached Iced `Theme` for widget styling.
    iced_theme: Theme,
}

impl InboxlyTheme {
    /// Create a theme from the given colour tokens.
    pub fn new(colors: ThemeColors) -> Self {
        let iced_theme = Self::build_iced_theme(&colors);
        Self { colors, iced_theme }
    }

    /// BigTop light theme.
    pub fn light() -> Self {
        Self::new(ThemeColors::light())
    }

    /// BigTop dark theme.
    pub fn dark() -> Self {
        Self::new(ThemeColors::dark())
    }

    /// Get the Iced `Theme` for use in widget styling.
    pub fn iced_theme(&self) -> &Theme {
        &self.iced_theme
    }

    /// Convert to an owned Iced `Theme`.
    pub fn into_iced_theme(self) -> Theme {
        self.iced_theme
    }

    /// Build an Iced `Theme::Custom` from our colour tokens.
    ///
    /// Maps our tokens to Iced's `Palette`:
    /// - `background` → our `background`
    /// - `text` → our `text_primary`
    /// - `primary` → our `toolbar_inbox` (the app's primary accent)
    /// - `success` → our `toolbar_done` (green for success actions)
    /// - `danger` → red from bundle category Social (used for delete/error)
    fn build_iced_theme(colors: &ThemeColors) -> Theme {
        let palette = Palette {
            background: colors.background,
            text: colors.text_primary,
            primary: colors.toolbar_inbox,
            success: colors.toolbar_done,
            danger: super::bundle_colors::SOCIAL.title,
        };

        let name = if colors.is_dark {
            "Inboxly Dark"
        } else {
            "Inboxly Light"
        };

        Theme::custom(name.to_string(), palette)
    }
}
```

**Note:** `Theme::custom(name, palette)` creates a `Theme::Custom(Arc<Custom>)` with automatic `Extended` palette generation from the base `Palette`. This is the recommended approach per Iced docs — it handles widget styling (button, text input, scrollbar, etc.) automatically from the 5 base palette colours.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement InboxlyTheme with Iced Theme integration`

---

## Task 11: System theme detection — D-Bus portal query

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme/system.rs` (new file)

Query `org.freedesktop.portal.Settings` for the system colour scheme preference. This is the standard freedesktop way to detect dark mode on both X11 and Wayland.

```rust
use tracing::{debug, warn};

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
pub async fn query_system_color_scheme() -> Result<SystemColorScheme, SystemThemeError> {
    let connection = zbus::Connection::session().await.map_err(|e| {
        debug!("D-Bus session connection failed: {e}");
        SystemThemeError::DBusUnavailable(e.to_string())
    })?;

    let proxy = zbus::proxy::Builder::new(&connection)
        .destination("org.freedesktop.portal.Desktop")
        .map_err(|e| SystemThemeError::DBusUnavailable(e.to_string()))?
        .path("/org/freedesktop/portal/desktop")
        .map_err(|e| SystemThemeError::DBusUnavailable(e.to_string()))?
        .interface("org.freedesktop.portal.Settings")
        .map_err(|e| SystemThemeError::DBusUnavailable(e.to_string()))?
        .build()
        .await
        .map_err(|e| {
            debug!("Failed to build portal proxy: {e}");
            SystemThemeError::DBusUnavailable(e.to_string())
        })?;

    // The Read method returns Variant<Variant<u32>> (double-wrapped).
    let reply: zbus::zvariant::OwnedValue = proxy
        .call_method("Read", &("org.freedesktop.appearance", "color-scheme"))
        .await
        .map_err(|e| {
            debug!("Portal Settings.Read call failed: {e}");
            SystemThemeError::PortalUnsupported(e.to_string())
        })?
        .body()
        .deserialize()
        .map_err(|e| {
            debug!("Failed to deserialize portal response: {e}");
            SystemThemeError::PortalUnsupported(e.to_string())
        })?;

    // Unwrap the double Variant wrapping.
    // The portal returns Variant[Variant[uint32]], we need to dig to the u32.
    let scheme = unwrap_color_scheme(&reply).unwrap_or_else(|| {
        warn!("Could not parse color-scheme value from portal, defaulting to NoPreference");
        0u32
    });

    let result = match scheme {
        1 => SystemColorScheme::Dark,
        2 => SystemColorScheme::Light,
        _ => SystemColorScheme::NoPreference,
    };

    debug!("System color scheme from portal: {result:?} (raw value: {scheme})");
    Ok(result)
}

/// Attempt to unwrap the double-variant color-scheme value to a u32.
fn unwrap_color_scheme(value: &zbus::zvariant::OwnedValue) -> Option<u32> {
    use zbus::zvariant::Value;

    // Try direct u32 first
    if let Ok(v) = <u32>::try_from(value) {
        return Some(v);
    }

    // Try single Variant unwrap
    if let Value::Value(inner) = value.downcast_ref()? {
        if let Ok(v) = <u32>::try_from(&**inner) {
            return Some(v);
        }
    }

    None
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
```

**Note:** Add `thiserror = "2"` to `inboxly-ui/Cargo.toml` if not already present. The `unwrap_color_scheme` helper handles the known double-variant wrapping issue documented in freedesktop portal bugs. The `zbus::proxy::Builder` approach avoids generating a full proxy trait — we only need one method call.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement system theme detection via freedesktop portal D-Bus`

---

## Task 12: Add system module to theme mod.rs

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme/mod.rs`

Add the `system` submodule to the module root and re-export key types.

Add to the module declarations:

```rust
pub mod system;
```

Add to re-exports:

```rust
pub use system::{query_system_color_scheme, SystemColorScheme, SystemThemeError};
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): export system theme detection from theme module`

---

## Task 13: `InboxlyTheme::from_system()` constructor

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme/mod.rs` (append to `impl InboxlyTheme`)

Add the `from_system()` async constructor that queries D-Bus and falls back to light.

```rust
impl InboxlyTheme {
    // ... (existing methods)

    /// Detect system theme preference and return the matching theme.
    ///
    /// Queries `org.freedesktop.portal.Settings` for `color-scheme`.
    /// Falls back to light theme if:
    /// - D-Bus is unavailable
    /// - The portal doesn't support the setting
    /// - The system reports "no preference"
    pub async fn from_system() -> Self {
        match system::query_system_color_scheme().await {
            Ok(SystemColorScheme::Dark) => {
                tracing::info!("System theme detected: dark");
                Self::dark()
            }
            Ok(SystemColorScheme::Light | SystemColorScheme::NoPreference) => {
                tracing::info!("System theme detected: light (or no preference)");
                Self::light()
            }
            Err(e) => {
                tracing::warn!("System theme detection failed ({e}), defaulting to light");
                Self::light()
            }
        }
    }
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add InboxlyTheme::from_system() with D-Bus detection`

---

## Task 14: Theme resolution from config preference

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme/mod.rs` (append to `impl InboxlyTheme`)

Add `from_preference()` which respects the user's `ThemePreference` from `inboxly-core::config`. This is the main entry point used at app startup.

```rust
use inboxly_core::config::ThemePreference;

impl InboxlyTheme {
    // ... (existing methods)

    /// Resolve theme from user preference.
    ///
    /// - `ThemePreference::Light` → light theme
    /// - `ThemePreference::Dark` → dark theme
    /// - `ThemePreference::System` → queries D-Bus portal (async)
    pub async fn from_preference(pref: ThemePreference) -> Self {
        match pref {
            ThemePreference::Light => Self::light(),
            ThemePreference::Dark => Self::dark(),
            ThemePreference::System => Self::from_system().await,
        }
    }

    /// Resolve theme from a preference stored in the settings table.
    ///
    /// Reads `"theme"` key from the settings store. If not found or
    /// not parseable, falls back to `ThemePreference::System`.
    pub async fn from_settings(settings: &dyn SettingsReader) -> Self {
        let pref = settings
            .get_setting("theme")
            .ok()
            .flatten()
            .and_then(|v| match v.as_str() {
                "light" => Some(ThemePreference::Light),
                "dark" => Some(ThemePreference::Dark),
                "system" => Some(ThemePreference::System),
                _ => None,
            })
            .unwrap_or(ThemePreference::System);

        Self::from_preference(pref).await
    }
}

/// Trait for reading settings — abstracts over the SQLite store.
///
/// Allows theme resolution without a direct dependency on the concrete
/// `inboxly-store` database type, enabling testing with mock stores.
pub trait SettingsReader {
    /// Read a setting value by key. Returns `None` if the key doesn't exist.
    fn get_setting(&self, key: &str) -> Result<Option<String>, Box<dyn std::error::Error>>;
}
```

**Note:** The `SettingsReader` trait is a thin abstraction. The `inboxly-store` crate's `SettingsStore` (from M3) will implement this trait. This keeps `inboxly-ui` from depending on SQLite directly for testing.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add theme resolution from preference and settings store`

---

## Task 15: Theme toggle — persist preference to settings

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme/mod.rs` (append to `impl InboxlyTheme`)

Add a method to persist theme changes. This is used when the user manually toggles theme in settings.

```rust
/// Trait for writing settings — abstracts over the SQLite store.
pub trait SettingsWriter {
    /// Write a setting key-value pair. Upserts (insert or replace).
    fn set_setting(&self, key: &str, value: &str) -> Result<(), Box<dyn std::error::Error>>;
}

impl InboxlyTheme {
    // ... (existing methods)

    /// Toggle between light and dark themes.
    ///
    /// Returns the new theme. Does not persist — call `save_preference` afterwards.
    pub fn toggle(&self) -> Self {
        if self.colors.is_dark {
            Self::light()
        } else {
            Self::dark()
        }
    }

    /// Save the current theme preference to the settings store.
    ///
    /// Stores `"light"` or `"dark"` under the `"theme"` key.
    /// When the user explicitly picks a theme, this overrides system detection.
    pub fn save_preference(
        &self,
        settings: &dyn SettingsWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let value = if self.colors.is_dark { "dark" } else { "light" };
        settings.set_setting("theme", value)
    }

    /// Reset to system theme detection (removes manual override).
    pub fn reset_to_system(
        settings: &dyn SettingsWriter,
    ) -> Result<(), Box<dyn std::error::Error>> {
        settings.set_setting("theme", "system")
    }
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add theme toggle and settings persistence`

---

## Task 16: Integrate theme into InboxlyApp

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` (or wherever M15 defined `InboxlyApp`)

Add the `InboxlyTheme` field to the app struct and wire it into Iced's `theme()` method. This task modifies the existing M15 code.

Add a `theme` field to `InboxlyApp`:

```rust
use crate::theme::InboxlyTheme;

pub struct InboxlyApp {
    // ... existing fields from M15 ...
    theme: InboxlyTheme,
}
```

In the app's initialization (wherever `InboxlyApp` is constructed), resolve the theme:

```rust
// During app init, before entering Iced's event loop:
// If running inside an async context (Iced's subscription or command):
let theme = InboxlyTheme::from_preference(config.theme).await;

// Or synchronously for first render, with system detection as a startup command:
let theme = match config.theme {
    ThemePreference::Light => InboxlyTheme::light(),
    ThemePreference::Dark => InboxlyTheme::dark(),
    ThemePreference::System => InboxlyTheme::light(), // temporary, updated by startup command
};
```

Wire the theme into Iced's application trait. The exact integration depends on whether M15 uses `iced::application()` builder or the `Application` trait:

**If using the builder pattern (Iced 0.13+):**

```rust
// In the Iced application builder:
iced::application("Inboxly", InboxlyApp::update, InboxlyApp::view)
    .theme(InboxlyApp::theme)
    // ...
    .run()
```

```rust
impl InboxlyApp {
    fn theme(&self) -> Theme {
        self.theme.iced_theme().clone()
    }
}
```

**If using the Application trait:**

```rust
impl Application for InboxlyApp {
    // ...
    fn theme(&self) -> Theme {
        self.theme.iced_theme().clone()
    }
}
```

Add a `Message` variant for theme changes:

```rust
pub enum Message {
    // ... existing variants from M15 ...
    ThemeToggled,
    ThemeChanged(InboxlyTheme),
}
```

Handle in `update`:

```rust
Message::ThemeToggled => {
    self.theme = self.theme.toggle();
    // Persist asynchronously — fire and forget
    if let Some(ref settings) = self.settings_writer {
        let _ = self.theme.save_preference(settings.as_ref());
    }
}
Message::ThemeChanged(new_theme) => {
    self.theme = new_theme;
}
```

**Note:** The exact field names and structure of `InboxlyApp` depend on what M15 delivers. The implementer should adapt this to the actual M15 code. The key integration points are: (1) `theme` field on the app struct, (2) returning the Iced theme from the `theme()` method, (3) handling `ThemeToggled` messages.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): integrate InboxlyTheme into Iced application`

---

## Task 17: Startup command for async system theme detection

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` (modify)

When the config says `ThemePreference::System`, the initial theme is light (synchronous), and we fire an async `Command` to query D-Bus and update the theme. This avoids blocking app startup on D-Bus.

Add to the app's startup/init:

```rust
use iced::Task;
use inboxly_core::config::ThemePreference;

impl InboxlyApp {
    // Called during app construction or as the initial command
    fn startup_commands(&self, pref: ThemePreference) -> Task<Message> {
        match pref {
            ThemePreference::System => {
                Task::perform(
                    InboxlyTheme::from_system(),
                    Message::ThemeChanged,
                )
            }
            _ => Task::none(),
        }
    }
}
```

**Note:** In Iced 0.13, `Command` was renamed to `Task`. The implementer should use whichever name the project's Iced version uses. This command runs asynchronously on the Iced runtime; when it completes, `Message::ThemeChanged` fires with the resolved theme.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add async system theme detection on startup`

---

## Task 18: Tests — ThemeColors light/dark values

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme/colors.rs` (append at end)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: assert a Color matches expected hex RGB values.
    fn assert_color_hex(color: Color, hex_str: &str) {
        let expected = hex(hex_str);
        assert!(
            (color.r - expected.r).abs() < 0.002
                && (color.g - expected.g).abs() < 0.002
                && (color.b - expected.b).abs() < 0.002,
            "expected {hex_str} ({expected:?}), got {color:?}"
        );
    }

    #[test]
    fn light_theme_background() {
        assert_color_hex(ThemeColors::light().background, "#ececec");
    }

    #[test]
    fn light_theme_surface() {
        assert_color_hex(ThemeColors::light().surface, "#ffffff");
    }

    #[test]
    fn light_theme_surface_selected() {
        assert_color_hex(ThemeColors::light().surface_selected, "#ebf2ff");
    }

    #[test]
    fn light_theme_text_primary() {
        assert_color_hex(ThemeColors::light().text_primary, "#212121");
    }

    #[test]
    fn light_theme_text_secondary() {
        assert_color_hex(ThemeColors::light().text_secondary, "#757575");
    }

    #[test]
    fn light_theme_divider() {
        assert_color_hex(ThemeColors::light().divider, "#e0e0e0");
    }

    #[test]
    fn light_theme_toolbar_inbox() {
        assert_color_hex(ThemeColors::light().toolbar_inbox, "#4285f4");
    }

    #[test]
    fn light_theme_toolbar_done() {
        assert_color_hex(ThemeColors::light().toolbar_done, "#0f9d58");
    }

    #[test]
    fn light_theme_toolbar_snoozed() {
        assert_color_hex(ThemeColors::light().toolbar_snoozed, "#ef6c00");
    }

    #[test]
    fn light_theme_is_not_dark() {
        assert!(!ThemeColors::light().is_dark);
    }

    #[test]
    fn dark_theme_background() {
        assert_color_hex(ThemeColors::dark().background, "#121212");
    }

    #[test]
    fn dark_theme_surface() {
        assert_color_hex(ThemeColors::dark().surface, "#1e1e1e");
    }

    #[test]
    fn dark_theme_surface_selected() {
        assert_color_hex(ThemeColors::dark().surface_selected, "#1a2744");
    }

    #[test]
    fn dark_theme_text_primary() {
        assert_color_hex(ThemeColors::dark().text_primary, "#e0e0e0");
    }

    #[test]
    fn dark_theme_text_secondary() {
        assert_color_hex(ThemeColors::dark().text_secondary, "#9e9e9e");
    }

    #[test]
    fn dark_theme_divider() {
        assert_color_hex(ThemeColors::dark().divider, "#2c2c2c");
    }

    #[test]
    fn dark_theme_toolbar_inbox() {
        assert_color_hex(ThemeColors::dark().toolbar_inbox, "#1a3a6e");
    }

    #[test]
    fn dark_theme_toolbar_done() {
        assert_color_hex(ThemeColors::dark().toolbar_done, "#0b5e35");
    }

    #[test]
    fn dark_theme_toolbar_snoozed() {
        assert_color_hex(ThemeColors::dark().toolbar_snoozed, "#8f4100");
    }

    #[test]
    fn dark_theme_is_dark() {
        assert!(ThemeColors::dark().is_dark);
    }

    #[test]
    fn hex_parser_lowercase() {
        let c = hex("#ff8000");
        assert!((c.r - 1.0).abs() < 0.002);
        assert!((c.g - 0.502).abs() < 0.002);
        assert!((c.b - 0.0).abs() < 0.002);
    }

    #[test]
    fn hex_parser_uppercase() {
        let c = hex("#FF8000");
        assert!((c.r - 1.0).abs() < 0.002);
        assert!((c.g - 0.502).abs() < 0.002);
        assert!((c.b - 0.0).abs() < 0.002);
    }

    #[test]
    fn hex_parser_no_hash() {
        let c = hex("4285f4");
        assert_color_hex(c, "#4285f4");
    }
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui -- theme::colors::tests
```

**Commit:** `test(ui): add ThemeColors light/dark value verification tests`

---

## Task 19: Tests — bundle category colours

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme/bundle_colors.rs` (append at end)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::colors::hex;

    fn assert_color_eq(a: Color, hex_str: &str) {
        let b = hex(hex_str);
        assert!(
            (a.r - b.r).abs() < 0.002
                && (a.g - b.g).abs() < 0.002
                && (a.b - b.b).abs() < 0.002,
            "expected {hex_str}, got {a:?}"
        );
    }

    #[test]
    fn social_title_is_red() {
        assert_color_eq(SOCIAL.title, "#d23f31");
    }

    #[test]
    fn social_badge_is_pink() {
        assert_color_eq(SOCIAL.badge, "#faebea");
    }

    #[test]
    fn promos_title_is_cyan() {
        assert_color_eq(PROMOS.title, "#00acc1");
    }

    #[test]
    fn updates_title_is_deep_orange() {
        assert_color_eq(UPDATES.title, "#f4511e");
    }

    #[test]
    fn finance_title_is_green() {
        assert_color_eq(FINANCE.title, "#558b2f");
    }

    #[test]
    fn purchases_title_is_brown() {
        assert_color_eq(PURCHASES.title, "#6d4c41");
    }

    #[test]
    fn travel_title_is_purple() {
        assert_color_eq(TRAVEL.title, "#8e24aa");
    }

    #[test]
    fn forums_title_is_indigo() {
        assert_color_eq(FORUMS.title, "#3949ab");
    }

    #[test]
    fn low_priority_title_is_dark_grey() {
        assert_color_eq(LOW_PRIORITY.title, "#212121");
    }

    #[test]
    fn all_eight_categories_have_distinct_titles() {
        let colors = [
            SOCIAL.title, PROMOS.title, UPDATES.title, FINANCE.title,
            PURCHASES.title, TRAVEL.title, FORUMS.title, LOW_PRIORITY.title,
        ];
        // Check no duplicates (compare as rgb tuples)
        for i in 0..colors.len() {
            for j in (i + 1)..colors.len() {
                assert!(
                    (colors[i].r - colors[j].r).abs() > 0.01
                        || (colors[i].g - colors[j].g).abs() > 0.01
                        || (colors[i].b - colors[j].b).abs() > 0.01,
                    "categories {i} and {j} have identical title colours"
                );
            }
        }
    }
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui -- theme::bundle_colors::tests
```

**Commit:** `test(ui): add bundle category colour verification tests`

---

## Task 20: Tests — avatar letter tile colours

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme/avatar_colors.rs` (append at end)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::colors::hex;

    fn assert_color_eq(a: Color, hex_str: &str) {
        let b = hex(hex_str);
        assert!(
            (a.r - b.r).abs() < 0.002
                && (a.g - b.g).abs() < 0.002
                && (a.b - b.b).abs() < 0.002,
            "expected {hex_str}, got {a:?}"
        );
    }

    #[test]
    fn avatar_a_is_red() {
        assert_color_eq(for_letter('A'), "#e06055");
    }

    #[test]
    fn avatar_z_is_green() {
        assert_color_eq(for_letter('Z'), "#aed581");
    }

    #[test]
    fn avatar_case_insensitive() {
        let upper = for_letter('F');
        let lower = for_letter('f');
        assert!((upper.r - lower.r).abs() < 0.001);
        assert!((upper.g - lower.g).abs() < 0.001);
        assert!((upper.b - lower.b).abs() < 0.001);
    }

    #[test]
    fn avatar_non_letter_returns_default() {
        assert_color_eq(for_letter('1'), "#efefef");
        assert_color_eq(for_letter('!'), "#efefef");
        assert_color_eq(for_letter(' '), "#efefef");
    }

    #[test]
    fn avatar_array_has_27_entries() {
        assert_eq!(AVATAR_COLORS.len(), 27); // 26 letters + default
    }

    #[test]
    fn avatar_m_is_yellow() {
        assert_color_eq(for_letter('M'), "#fdd835");
    }

    #[test]
    fn avatar_all_26_letters_resolve() {
        for c in b'A'..=b'Z' {
            let color = for_letter(c as char);
            // Should not be the default colour (index 26)
            let default = AVATAR_COLORS[26];
            // Q and W happen to equal default grey, skip those
            if c != b'Q' && c != b'W' {
                assert!(
                    (color.r - default.r).abs() > 0.01
                        || (color.g - default.g).abs() > 0.01
                        || (color.b - default.b).abs() > 0.01,
                    "letter {} returned default colour",
                    c as char
                );
            }
        }
    }
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui -- theme::avatar_colors::tests
```

**Commit:** `test(ui): add avatar letter tile colour tests`

---

## Task 21: Tests — InboxlyTheme construction and Iced integration

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme/mod.rs` (append at end of file)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_theme_creates_successfully() {
        let theme = InboxlyTheme::light();
        assert!(!theme.colors.is_dark);
    }

    #[test]
    fn dark_theme_creates_successfully() {
        let theme = InboxlyTheme::dark();
        assert!(theme.colors.is_dark);
    }

    #[test]
    fn toggle_light_to_dark() {
        let light = InboxlyTheme::light();
        let dark = light.toggle();
        assert!(dark.colors.is_dark);
    }

    #[test]
    fn toggle_dark_to_light() {
        let dark = InboxlyTheme::dark();
        let light = dark.toggle();
        assert!(!light.colors.is_dark);
    }

    #[test]
    fn toggle_is_involution() {
        let original = InboxlyTheme::light();
        let toggled_twice = original.toggle().toggle();
        assert_eq!(original.colors.is_dark, toggled_twice.colors.is_dark);
    }

    #[test]
    fn iced_theme_is_custom_variant() {
        let theme = InboxlyTheme::light();
        // Theme::Custom contains our palette — verify it's not a built-in variant
        let iced = theme.iced_theme();
        // The palette background should match our light background
        let palette = iced.palette();
        let bg = palette.background;
        let expected = ThemeColors::light().background;
        assert!(
            (bg.r - expected.r).abs() < 0.01
                && (bg.g - expected.g).abs() < 0.01
                && (bg.b - expected.b).abs() < 0.01,
            "Iced theme palette background doesn't match light theme"
        );
    }

    #[test]
    fn iced_theme_dark_palette() {
        let theme = InboxlyTheme::dark();
        let palette = theme.iced_theme().palette();
        let bg = palette.background;
        let expected = ThemeColors::dark().background;
        assert!(
            (bg.r - expected.r).abs() < 0.01
                && (bg.g - expected.g).abs() < 0.01
                && (bg.b - expected.b).abs() < 0.01,
            "Iced theme palette background doesn't match dark theme"
        );
    }

    #[test]
    fn iced_theme_primary_is_toolbar_inbox() {
        let theme = InboxlyTheme::light();
        let palette = theme.iced_theme().palette();
        let primary = palette.primary;
        let expected = ThemeColors::light().toolbar_inbox;
        assert!(
            (primary.r - expected.r).abs() < 0.01
                && (primary.g - expected.g).abs() < 0.01
                && (primary.b - expected.b).abs() < 0.01,
        );
    }

    /// Mock settings reader for testing.
    struct MockSettings {
        value: Option<String>,
    }

    impl SettingsReader for MockSettings {
        fn get_setting(&self, _key: &str) -> Result<Option<String>, Box<dyn std::error::Error>> {
            Ok(self.value.clone())
        }
    }

    impl SettingsWriter for MockSettings {
        fn set_setting(&self, _key: &str, _value: &str) -> Result<(), Box<dyn std::error::Error>> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn from_preference_light() {
        let theme = InboxlyTheme::from_preference(ThemePreference::Light).await;
        assert!(!theme.colors.is_dark);
    }

    #[tokio::test]
    async fn from_preference_dark() {
        let theme = InboxlyTheme::from_preference(ThemePreference::Dark).await;
        assert!(theme.colors.is_dark);
    }

    #[tokio::test]
    async fn from_settings_dark() {
        let settings = MockSettings {
            value: Some("dark".to_string()),
        };
        let theme = InboxlyTheme::from_settings(&settings).await;
        assert!(theme.colors.is_dark);
    }

    #[tokio::test]
    async fn from_settings_light() {
        let settings = MockSettings {
            value: Some("light".to_string()),
        };
        let theme = InboxlyTheme::from_settings(&settings).await;
        assert!(!theme.colors.is_dark);
    }

    #[tokio::test]
    async fn from_settings_missing_key_defaults_to_system() {
        let settings = MockSettings { value: None };
        // System detection may or may not work in CI, but the function should not panic
        let _theme = InboxlyTheme::from_settings(&settings).await;
    }

    #[tokio::test]
    async fn from_settings_invalid_value_defaults_to_system() {
        let settings = MockSettings {
            value: Some("purple".to_string()),
        };
        let _theme = InboxlyTheme::from_settings(&settings).await;
    }
}
```

**Note:** Add `tokio = { version = "1", features = ["macros", "rt-multi-thread"] }` to `[dev-dependencies]` in `inboxly-ui/Cargo.toml` if not already present.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui -- theme::tests
```

**Commit:** `test(ui): add InboxlyTheme construction and integration tests`

---

## Task 22: Tests — dimension and typography constants

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme/dimensions.rs` (append at end)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toolbar_height_is_56dp() {
        assert_eq!(TOOLBAR_HEIGHT, 56.0);
    }

    #[test]
    fn nav_drawer_width_is_264dp() {
        assert_eq!(NAV_DRAWER_WIDTH, 264.0);
    }

    #[test]
    fn avatar_diameter_is_40dp() {
        assert_eq!(AVATAR_DIAMETER, 40.0);
    }

    #[test]
    fn fab_diameter_is_56dp() {
        assert_eq!(FAB_DIAMETER, 56.0);
    }

    #[test]
    fn default_padding_is_16dp() {
        assert_eq!(DEFAULT_PADDING, 16.0);
    }

    #[test]
    fn snooze_grid_width_is_288dp() {
        assert_eq!(SNOOZE_GRID_WIDTH, 288.0);
    }

    #[test]
    fn compose_max_width_is_920dp() {
        assert_eq!(COMPOSE_MAX_WIDTH, 920.0);
    }

    #[test]
    fn divider_is_1px() {
        assert_eq!(DIVIDER_THICKNESS, 1.0);
    }

    #[test]
    fn flat_cards_have_zero_radius() {
        assert_eq!(LIST_ITEM_CORNER_RADIUS, 0.0);
    }
}
```

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme/typography.rs` (append at end)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use iced::font::Weight;

    #[test]
    fn toolbar_title_is_20sp() {
        assert_eq!(TOOLBAR_TITLE_SIZE, 20.0);
    }

    #[test]
    fn email_title_is_16sp() {
        assert_eq!(EMAIL_TITLE_SIZE, 16.0);
    }

    #[test]
    fn snippet_is_14sp() {
        assert_eq!(SNIPPET_SIZE, 14.0);
    }

    #[test]
    fn timestamp_is_12sp() {
        assert_eq!(TIMESTAMP_SIZE, 12.0);
    }

    #[test]
    fn section_header_is_bold() {
        assert!(matches!(SECTION_HEADER_WEIGHT, Weight::Bold));
    }

    #[test]
    fn unread_title_is_bold() {
        assert!(matches!(EMAIL_TITLE_WEIGHT_UNREAD, Weight::Bold));
    }

    #[test]
    fn read_title_is_normal() {
        assert!(matches!(EMAIL_TITLE_WEIGHT, Weight::Normal));
    }

    #[test]
    fn nav_item_is_medium_weight() {
        assert!(matches!(NAV_ITEM_WEIGHT, Weight::Medium));
    }

    #[test]
    fn compose_subject_is_18sp_bold() {
        assert_eq!(COMPOSE_SUBJECT_SIZE, 18.0);
        assert!(matches!(COMPOSE_SUBJECT_WEIGHT, Weight::Bold));
    }
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui -- theme::dimensions::tests theme::typography::tests
```

**Commit:** `test(ui): add dimension and typography constant verification tests`

---

## Task 23: Final verification and clippy

Run the full test suite and clippy to confirm everything is clean.

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui && cargo clippy -p inboxly-ui -- -D warnings
```

If any clippy warnings exist, fix them. Then do a final commit if any fixes were needed:

**Commit (if needed):** `fix(ui): address clippy warnings in theme module`

---

## Summary

After all 23 tasks, the `inboxly-ui` crate's theme module contains:

| File | Contents |
|------|----------|
| `src/theme/mod.rs` | `InboxlyTheme` struct, `light()`/`dark()`/`from_system()`/`from_preference()`/`from_settings()`, toggle/persist, `SettingsReader`/`SettingsWriter` traits, Iced `Theme::Custom` integration |
| `src/theme/colors.rs` | `ThemeColors` struct with all colour tokens, `const fn hex()` parser, `light()`/`dark()` constructors |
| `src/theme/bundle_colors.rs` | 8 `BundleCategoryColor` constants, `for_category()` lookup |
| `src/theme/avatar_colors.rs` | 27-entry `AVATAR_COLORS` array (A-Z + default), `for_letter()` lookup |
| `src/theme/dimensions.rs` | 16 dimension constants (toolbar, nav, avatar, FAB, snooze, compose, divider) |
| `src/theme/typography.rs` | 20 typography constants (sizes + weights for all UI elements) |
| `src/theme/system.rs` | `query_system_color_scheme()` async D-Bus query, `SystemColorScheme` enum, error types |

**Total tests:** ~55 (22 colour token + 11 bundle + 7 avatar + 9 dimensions + 9 typography + 12 InboxlyTheme integration + mock settings)

**Dependencies added:** `zbus 5`, `thiserror 2`, `tracing 0.1` (tokio for dev-deps)

**Integration points:**
- `InboxlyApp.theme` field holds the active theme
- `InboxlyApp::theme()` returns `iced::Theme` for widget styling
- `Message::ThemeToggled` handles manual toggle
- `Message::ThemeChanged` handles async system detection result
- Settings persistence via `SettingsReader`/`SettingsWriter` traits (implemented by M3's store)
