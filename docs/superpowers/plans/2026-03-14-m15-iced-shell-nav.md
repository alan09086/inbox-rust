# M15: Iced Shell + Nav Drawer — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan.

**Goal:** Create the Inboxly application window with navigation drawer, toolbar, and view switching.

**Architecture:** Iced elm-architecture: Model (app state) → Message (events) → Update (state changes) → View (render). Nav drawer is always visible on desktop. The toolbar colour changes based on the active view (blue/orange/green for Inbox/Snoozed/Done). All UI code lives in `inboxly-ui`; the binary crate `inboxly` just launches the app.

**Tech Stack:** Rust, iced (0.13+), inboxly-core

**Prerequisites:**
- M1 complete — `inboxly-core` crate exists with core types (`BundleCategory`, `BundleId`, colour types)
- M3 complete — `inboxly-store` crate exists (we import the `BundleCategory` enum from core, but don't query the store yet — the nav drawer bundle list is hardcoded for this milestone)

---

## Task 1: Create `inboxly-ui` crate with Iced dependency

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/Cargo.toml` (new file)

```toml
[package]
name = "inboxly-ui"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
inboxly-core = { path = "../inboxly-core" }
iced = { version = "0.13", features = ["advanced", "canvas"] }
```

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/lib.rs` (new file)

```rust
pub mod app;
pub mod theme;
pub mod nav;
pub mod toolbar;
```

**File:** `/mnt/TempNVME/projects/inbox-rust/Cargo.toml` (edit — add `inboxly-ui` to workspace members)

Add `"inboxly-ui"` to the `members` list in the `[workspace]` section.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): create inboxly-ui crate with iced dependency`

---

## Task 2: Define ActiveView enum and colour constants

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme.rs` (new file)

Define the three primary views and their associated toolbar colours. These map directly to the spec's View States table and Theme System colour tokens.

```rust
use iced::Color;

/// The three primary views that drive toolbar colour and content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ActiveView {
    #[default]
    Inbox,
    Snoozed,
    Done,
}

impl ActiveView {
    /// Display name shown in the toolbar title.
    pub fn title(&self) -> &'static str {
        match self {
            Self::Inbox => "Inbox",
            Self::Snoozed => "Snoozed",
            Self::Done => "Done",
        }
    }

    /// Toolbar background colour for this view (light theme).
    pub fn toolbar_color(&self) -> Color {
        match self {
            Self::Inbox => color_from_hex(0x42, 0x85, 0xf4),   // #4285f4
            Self::Snoozed => color_from_hex(0xef, 0x6c, 0x00), // #ef6c00
            Self::Done => color_from_hex(0x0f, 0x9d, 0x58),    // #0f9d58
        }
    }
}

/// Convert RGB bytes to iced::Color (0.0..1.0 range).
pub fn color_from_hex(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}

// === Layout constants (from BigTop APK, in logical pixels) ===
pub const TOOLBAR_HEIGHT: f32 = 56.0;
pub const NAV_DRAWER_WIDTH: f32 = 264.0;
pub const NAV_ITEM_HEIGHT: f32 = 48.0;
pub const AVATAR_DIAMETER: f32 = 40.0;
pub const DEFAULT_PADDING: f32 = 16.0;
pub const DIVIDER_THICKNESS: f32 = 1.0;

// === Theme colours (light) ===
pub const BG_COLOR: Color = Color::from_rgb(0xec as f32 / 255.0, 0xec as f32 / 255.0, 0xec as f32 / 255.0); // #ececec
pub const SURFACE_COLOR: Color = Color::from_rgb(1.0, 1.0, 1.0); // #ffffff
pub const PRIMARY_TEXT: Color = Color::from_rgb(0x21 as f32 / 255.0, 0x21 as f32 / 255.0, 0x21 as f32 / 255.0); // #212121
pub const SECONDARY_TEXT: Color = Color::from_rgb(0x75 as f32 / 255.0, 0x75 as f32 / 255.0, 0x75 as f32 / 255.0); // #757575
pub const DIVIDER_COLOR: Color = Color::from_rgb(0xe0 as f32 / 255.0, 0xe0 as f32 / 255.0, 0xe0 as f32 / 255.0); // #e0e0e0
pub const SELECTED_BG: Color = Color::from_rgb(0xeb as f32 / 255.0, 0xf2 as f32 / 255.0, 0xff as f32 / 255.0); // #ebf2ff

// === Typography (in sp / logical pixels) ===
pub const TOOLBAR_TITLE_SIZE: f32 = 20.0;
pub const NAV_ITEM_SIZE: f32 = 14.0;
pub const AUTHOR_SIZE: f32 = 14.0;
pub const TIMESTAMP_SIZE: f32 = 12.0;

// === Bundle category colours (title colour from spec) ===
pub struct CategoryColor {
    pub title: Color,
    pub badge: Color,
}

pub fn category_color(category: &str) -> CategoryColor {
    match category {
        "Social" => CategoryColor {
            title: color_from_hex(0xd2, 0x3f, 0x31),
            badge: color_from_hex(0xfa, 0xeb, 0xea),
        },
        "Promos" => CategoryColor {
            title: color_from_hex(0x00, 0xac, 0xc1),
            badge: color_from_hex(0xe5, 0xf6, 0xf9),
        },
        "Updates" => CategoryColor {
            title: color_from_hex(0xf4, 0x51, 0x1e),
            badge: color_from_hex(0xfe, 0xed, 0xe8),
        },
        "Finance" => CategoryColor {
            title: color_from_hex(0x55, 0x8b, 0x2f),
            badge: color_from_hex(0xee, 0xf3, 0xea),
        },
        "Purchases" => CategoryColor {
            title: color_from_hex(0x6d, 0x4c, 0x41),
            badge: color_from_hex(0xf0, 0xed, 0xec),
        },
        "Travel" => CategoryColor {
            title: color_from_hex(0x8e, 0x24, 0xaa),
            badge: color_from_hex(0xf3, 0xe9, 0xf6),
        },
        "Forums" => CategoryColor {
            title: color_from_hex(0x39, 0x49, 0xab),
            badge: color_from_hex(0xeb, 0xec, 0xf6),
        },
        "Low Priority" => CategoryColor {
            title: color_from_hex(0x21, 0x21, 0x21),
            badge: color_from_hex(0xe5, 0xe5, 0xe5),
        },
        _ => CategoryColor {
            title: SECONDARY_TEXT,
            badge: DIVIDER_COLOR,
        },
    }
}
```

Note: Iced 0.13's `Color::from_rgb` in `const` context may require `from_rgb()` with float literals. If the compiler rejects the const expressions, convert BG_COLOR etc. to lazy_static or functions. The worker should test compilation and adjust.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add ActiveView enum, theme colours, and layout constants`

---

## Task 3: Define the nav drawer item types

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/nav.rs` (new file)

The nav drawer has three sections separated by dividers:
1. **Primary nav** — Inbox, Snoozed, Done (map to `ActiveView`)
2. **Secondary nav** — Drafts, Sent, Reminders, Trash, Spam (folder views)
3. **Bundle categories** — Social, Promos, Updates, etc. (with coloured dots)

Define `NavSection` as the secondary nav target and `NavTarget` as the unified nav destination.

```rust
use crate::theme::ActiveView;

/// Secondary navigation destinations (folders, not primary views).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NavSection {
    Drafts,
    Sent,
    Reminders,
    Trash,
    Spam,
}

impl NavSection {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Drafts => "Drafts",
            Self::Sent => "Sent",
            Self::Reminders => "Reminders",
            Self::Trash => "Trash",
            Self::Spam => "Spam",
        }
    }

    /// All secondary nav items in display order.
    pub fn all() -> &'static [NavSection] {
        &[
            Self::Drafts,
            Self::Sent,
            Self::Reminders,
            Self::Trash,
            Self::Spam,
        ]
    }
}

/// A bundle category entry for the nav drawer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavBundleCategory {
    pub name: String,
}

/// Default bundle categories shown in the nav drawer.
pub fn default_bundle_categories() -> Vec<NavBundleCategory> {
    vec![
        NavBundleCategory { name: "Social".into() },
        NavBundleCategory { name: "Promos".into() },
        NavBundleCategory { name: "Updates".into() },
        NavBundleCategory { name: "Finance".into() },
        NavBundleCategory { name: "Purchases".into() },
        NavBundleCategory { name: "Travel".into() },
        NavBundleCategory { name: "Forums".into() },
        NavBundleCategory { name: "Low Priority".into() },
    ]
}

/// Unified navigation target — any clickable item in the nav drawer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavTarget {
    /// Primary views (Inbox, Snoozed, Done) — changes toolbar colour.
    View(ActiveView),
    /// Secondary nav (Drafts, Sent, etc.) — loads folder content.
    Section(NavSection),
    /// Bundle category filter — shows emails in that bundle.
    BundleCategory(String),
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add nav drawer item types and default bundle categories`

---

## Task 4: Define the Message enum and Inboxly app state

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` (new file)

This is the core of the Iced elm architecture. Define the `Inboxly` struct (model) and `Message` enum (events).

```rust
use iced::widget::{column, container, row, text};
use iced::{Element, Length, Task, Theme};

use crate::nav::{default_bundle_categories, NavBundleCategory, NavSection, NavTarget};
use crate::theme::ActiveView;

/// Top-level application state.
pub struct Inboxly {
    /// Currently active primary view (drives toolbar colour).
    pub active_view: ActiveView,
    /// Currently selected nav target (may be a primary view, folder, or bundle).
    pub active_nav: NavTarget,
    /// Whether the nav drawer is visible (toggled by hamburger).
    pub drawer_open: bool,
    /// Bundle categories shown in the nav drawer.
    pub bundle_categories: Vec<NavBundleCategory>,
    /// Mock account info for the account switcher.
    pub account_email: String,
    /// Number of accounts (for the account switcher display).
    pub account_count: u32,
}

/// All messages the application can receive.
#[derive(Debug, Clone)]
pub enum Message {
    /// User clicked a nav item.
    Navigate(NavTarget),
    /// User toggled the hamburger menu.
    ToggleDrawer,
    /// Search bar was focused / text changed (placeholder for now).
    SearchChanged(String),
}

impl Default for Inboxly {
    fn default() -> Self {
        Self {
            active_view: ActiveView::Inbox,
            active_nav: NavTarget::View(ActiveView::Inbox),
            drawer_open: true,
            bundle_categories: default_bundle_categories(),
            account_email: "user@example.com".into(),
            account_count: 1,
        }
    }
}

impl Inboxly {
    /// Create the app with initial state. Returns (Self, Task).
    pub fn new() -> (Self, Task<Message>) {
        (Self::default(), Task::none())
    }

    /// Iced update function — handle messages and mutate state.
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Navigate(target) => {
                // If navigating to a primary view, update the toolbar colour.
                if let NavTarget::View(view) = &target {
                    self.active_view = *view;
                }
                self.active_nav = target;
            }
            Message::ToggleDrawer => {
                self.drawer_open = !self.drawer_open;
            }
            Message::SearchChanged(_query) => {
                // Placeholder — search is M24.
            }
        }
        Task::none()
    }

    /// Iced view function — render the entire UI.
    /// Delegates to toolbar::view_toolbar, nav::view_drawer, and a content placeholder.
    pub fn view(&self) -> Element<Message> {
        use crate::nav::view_drawer;
        use crate::toolbar::view_toolbar;

        let toolbar = view_toolbar(self);

        let drawer = if self.drawer_open {
            Some(view_drawer(self))
        } else {
            None
        };

        let content_area: Element<Message> = container(
            text(format!("{} — content area placeholder", self.active_view.title()))
                .size(16.0),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(crate::theme::DEFAULT_PADDING)
        .into();

        let body = match drawer {
            Some(drawer_el) => row![drawer_el, content_area]
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            None => content_area,
        };

        column![toolbar, body]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    /// Window title.
    pub fn title(&self) -> String {
        format!("Inboxly — {}", self.active_view.title())
    }

    /// Iced theme — using built-in light theme (custom theme is M16).
    pub fn theme(&self) -> Theme {
        Theme::Light
    }
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

This will fail because `view_drawer` and `view_toolbar` don't exist yet — that's expected. Tasks 5 and 6 will add them. The worker should stub them temporarily or implement tasks 4-6 together.

**Commit:** `feat(ui): define Inboxly app state and Message enum with elm-architecture update/view`

---

## Task 5: Implement the toolbar view

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/toolbar.rs` (new file)

The toolbar is 56dp tall, background colour changes by active view. Contains:
- Left: hamburger menu toggle button (☰ text for now — icon is M16/later polish)
- Center-left: title text (20sp) showing the active view name
- Center: search bar placeholder (a text input or styled container)
- Right: account avatar placeholder (40dp circle with first letter)

```rust
use iced::widget::{button, container, row, text, text_input, Space};
use iced::{Alignment, Background, Border, Color, Element, Length};

use crate::app::{Inboxly, Message};
use crate::theme::{
    AVATAR_DIAMETER, DEFAULT_PADDING, TOOLBAR_HEIGHT, TOOLBAR_TITLE_SIZE,
};

/// Render the toolbar bar.
pub fn view_toolbar(app: &Inboxly) -> Element<Message> {
    let toolbar_bg = app.active_view.toolbar_color();

    // Hamburger button
    let hamburger = button(text("☰").size(20.0))
        .on_press(Message::ToggleDrawer)
        .padding([8, 12])
        .style(move |_theme, _status| button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            text_color: Color::WHITE,
            border: Border::default(),
            ..Default::default()
        });

    // Title
    let title = text(app.active_view.title())
        .size(TOOLBAR_TITLE_SIZE)
        .color(Color::WHITE);

    // Search placeholder
    let search = text_input("Search mail", "")
        .on_input(Message::SearchChanged)
        .width(Length::FillPortion(3))
        .padding([8, 12]);

    // Account avatar (first letter circle — placeholder)
    let avatar_letter = app
        .account_email
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();
    let avatar = container(
        text(avatar_letter)
            .size(16.0)
            .color(Color::WHITE)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center),
    )
    .width(AVATAR_DIAMETER)
    .height(AVATAR_DIAMETER)
    .align_x(iced::alignment::Horizontal::Center)
    .align_y(iced::alignment::Vertical::Center)
    .style(move |_theme| container::Style {
        background: Some(Background::Color(Color::from_rgb(0.4, 0.4, 0.4))),
        border: Border {
            radius: (AVATAR_DIAMETER / 2.0).into(),
            ..Default::default()
        },
        ..Default::default()
    });

    let toolbar_row = row![
        hamburger,
        title,
        Space::with_width(Length::Fixed(DEFAULT_PADDING)),
        search,
        Space::with_width(Length::Fill),
        avatar,
    ]
    .spacing(12)
    .padding([0, DEFAULT_PADDING])
    .align_y(Alignment::Center)
    .height(TOOLBAR_HEIGHT)
    .width(Length::Fill);

    container(toolbar_row)
        .width(Length::Fill)
        .height(TOOLBAR_HEIGHT)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(toolbar_bg)),
            ..Default::default()
        })
        .into()
}
```

**Important implementation notes for the worker:**
- Iced 0.13 API may differ from 0.12. Check `button::style()` closure signature — it may be `Fn(&Theme, button::Status) -> button::Style` or use a `StyleSheet` trait. Adjust to match the actual Iced 0.13 API.
- If `text_input::on_input` doesn't exist, use `text_input::on_change` or the equivalent.
- The toolbar should feel solid — a colored bar spanning the full width with white text on top.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement toolbar with hamburger, title, search, and avatar`

---

## Task 6: Implement the nav drawer view

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/nav.rs` (append to existing file from Task 3)

Add a `view_drawer` function that renders the full nav drawer as a 264dp-wide column. The drawer has:

1. **Account switcher** — avatar circle (40dp) + email text + account count indicator
2. **Divider**
3. **Primary nav** — Inbox (active by default), Snoozed, Done
4. **Divider**
5. **Secondary nav** — Drafts, Sent, Reminders, Trash, Spam
6. **Divider**
7. **Bundle categories** — coloured dot + category name, scrollable if needed

Each nav item is 48dp tall. The active item gets a `#ebf2ff` selected background. Clicking any item sends `Message::Navigate(NavTarget)`.

```rust
use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Alignment, Background, Border, Color, Element, Length};

use crate::app::{Inboxly, Message};
use crate::theme::{
    category_color, color_from_hex, ActiveView, DIVIDER_COLOR, DIVIDER_THICKNESS,
    NAV_DRAWER_WIDTH, NAV_ITEM_HEIGHT, NAV_ITEM_SIZE, DEFAULT_PADDING,
    AVATAR_DIAMETER, SECONDARY_TEXT, SELECTED_BG, SURFACE_COLOR, PRIMARY_TEXT,
};

/// Render a single nav item row (48dp tall, full width, selectable).
fn nav_item<'a>(
    label: &str,
    target: NavTarget,
    is_active: bool,
    dot_color: Option<Color>,
) -> Element<'a, Message> {
    let bg = if is_active { SELECTED_BG } else { SURFACE_COLOR };

    let mut content_row = row![].spacing(12).align_y(Alignment::Center);

    // Optional coloured dot for bundle categories
    if let Some(dot) = dot_color {
        let dot_widget = container(Space::new(8.0, 8.0))
            .style(move |_theme| container::Style {
                background: Some(Background::Color(dot)),
                border: Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            });
        content_row = content_row.push(dot_widget);
    }

    content_row = content_row.push(
        text(label)
            .size(NAV_ITEM_SIZE)
            .color(if is_active {
                color_from_hex(0x42, 0x85, 0xf4) // blue for active
            } else {
                PRIMARY_TEXT
            }),
    );

    let btn = button(
        container(content_row)
            .padding([0, DEFAULT_PADDING])
            .height(NAV_ITEM_HEIGHT)
            .width(Length::Fill)
            .align_y(iced::alignment::Vertical::Center),
    )
    .on_press(Message::Navigate(target))
    .width(Length::Fill)
    .style(move |_theme, _status| button::Style {
        background: Some(Background::Color(bg)),
        text_color: PRIMARY_TEXT,
        border: Border::default(),
        ..Default::default()
    });

    btn.into()
}

/// Render a horizontal divider line.
fn divider<'a>() -> Element<'a, Message> {
    container(Space::new(Length::Fill, DIVIDER_THICKNESS))
        .width(Length::Fill)
        .style(|_theme| container::Style {
            background: Some(Background::Color(DIVIDER_COLOR)),
            ..Default::default()
        })
        .into()
}

/// Render the account switcher at the top of the nav drawer.
fn account_switcher<'a>(email: &str, account_count: u32) -> Element<'a, Message> {
    let avatar_letter = email
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();

    let avatar = container(
        text(avatar_letter)
            .size(18.0)
            .color(Color::WHITE)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center),
    )
    .width(AVATAR_DIAMETER)
    .height(AVATAR_DIAMETER)
    .align_x(iced::alignment::Horizontal::Center)
    .align_y(iced::alignment::Vertical::Center)
    .style(|_theme| container::Style {
        background: Some(Background::Color(color_from_hex(0x42, 0x85, 0xf4))),
        border: Border {
            radius: (AVATAR_DIAMETER / 2.0).into(),
            ..Default::default()
        },
        ..Default::default()
    });

    let email_text = text(email.to_string())
        .size(NAV_ITEM_SIZE)
        .color(PRIMARY_TEXT);

    let count_text = text(format!("{} account{}", account_count, if account_count != 1 { "s" } else { "" }))
        .size(12.0)
        .color(SECONDARY_TEXT);

    let info_col = column![email_text, count_text].spacing(2);

    container(
        row![avatar, info_col]
            .spacing(12)
            .align_y(Alignment::Center)
            .padding(DEFAULT_PADDING),
    )
    .width(Length::Fill)
    .into()
}

/// Render the full nav drawer (264dp wide).
pub fn view_drawer(app: &Inboxly) -> Element<Message> {
    let mut drawer = column![].width(NAV_DRAWER_WIDTH);

    // Account switcher
    drawer = drawer.push(account_switcher(&app.account_email, app.account_count));
    drawer = drawer.push(divider());

    // Primary nav: Inbox, Snoozed, Done
    for view in &[ActiveView::Inbox, ActiveView::Snoozed, ActiveView::Done] {
        let target = NavTarget::View(*view);
        let is_active = app.active_nav == target;
        drawer = drawer.push(nav_item(view.title(), target, is_active, None));
    }

    drawer = drawer.push(divider());

    // Secondary nav: Drafts, Sent, Reminders, Trash, Spam
    for section in NavSection::all() {
        let target = NavTarget::Section(*section);
        let is_active = app.active_nav == target;
        drawer = drawer.push(nav_item(section.label(), target, is_active, None));
    }

    drawer = drawer.push(divider());

    // Bundle categories section header
    drawer = drawer.push(
        container(
            text("Bundles")
                .size(12.0)
                .color(SECONDARY_TEXT),
        )
        .padding([12, DEFAULT_PADDING])
        .width(Length::Fill),
    );

    // Bundle category items with coloured dots
    let mut bundle_col = column![];
    for cat in &app.bundle_categories {
        let target = NavTarget::BundleCategory(cat.name.clone());
        let is_active = app.active_nav == target;
        let dot = category_color(&cat.name).title;
        bundle_col = bundle_col.push(nav_item(&cat.name, target, is_active, Some(dot)));
    }

    // Wrap bundle categories in a scrollable in case the list is long
    drawer = drawer.push(scrollable(bundle_col).height(Length::Fill));

    container(drawer)
        .width(NAV_DRAWER_WIDTH)
        .height(Length::Fill)
        .style(|_theme| container::Style {
            background: Some(Background::Color(SURFACE_COLOR)),
            ..Default::default()
        })
        .into()
}
```

**Important implementation notes for the worker:**
- The `nav_item` function must handle ownership carefully — `label` and `target` are consumed by the closure/button. Use `.to_string()` or clone as needed.
- Iced 0.13 scrollable API: check if `scrollable()` wraps a `Column` directly or needs `Scrollable::new(content)`.
- The drawer background is white (`#ffffff` / `SURFACE_COLOR`) with the app background behind it being `#ececec`.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement nav drawer with account switcher, primary/secondary nav, and bundle categories`

---

## Task 7: Create the binary crate that launches the app

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly/Cargo.toml` (new file)

```toml
[package]
name = "inboxly"
version.workspace = true
edition.workspace = true
license.workspace = true

[[bin]]
name = "inboxly"
path = "src/main.rs"

[dependencies]
inboxly-ui = { path = "../inboxly-ui" }
iced = { version = "0.13", features = ["advanced"] }
```

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly/src/main.rs` (new file)

```rust
use iced::{self, Size};
use inboxly_ui::app::{Inboxly, Message};

fn main() -> iced::Result {
    iced::application(Inboxly::title, Inboxly::update, Inboxly::view)
        .window_size(Size::new(1280.0, 800.0))
        .theme(Inboxly::theme)
        .run_with(Inboxly::new)
}
```

**Also:** Add `"inboxly"` to the workspace `members` list in the root `Cargo.toml`.

**Important implementation notes for the worker:**
- Iced 0.13 uses the `iced::application()` builder function (not `Application::run(Settings)`). Check the actual API — it may be `iced::application(title, update, view).run()` or similar.
- The `title`, `update`, and `view` function references must match the Iced 0.13 expected signatures: `title: fn(&Model) -> String`, `update: fn(&mut Model, Message) -> Task<Message>`, `view: fn(&Model) -> Element<Message>`.
- `window_size` sets the initial window dimensions. Adjust the method name if the API uses `window(iced::window::Settings { size: ... })` instead.
- The binary should compile and launch a window showing the toolbar + nav drawer + empty content area.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo build -p inboxly
```

Then manually launch to verify visual output:

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo run -p inboxly
```

Expected: a 1280x800 window with blue toolbar at top ("Inbox" title, hamburger, search bar, avatar), white nav drawer on left (264dp) with account switcher + nav items + bundle categories, and gray content area showing "Inbox — content area placeholder".

**Commit:** `feat: create inboxly binary crate that launches the Iced application`

---

## Task 8: Add state management tests

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` (append tests module)

UI rendering is difficult to unit test, but the elm-architecture update logic is pure state mutation — fully testable. Add tests verifying that `Message` → `update()` produces the expected state changes.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::nav::{NavSection, NavTarget};
    use crate::theme::ActiveView;

    #[test]
    fn default_state_is_inbox() {
        let app = Inboxly::default();
        assert_eq!(app.active_view, ActiveView::Inbox);
        assert_eq!(app.active_nav, NavTarget::View(ActiveView::Inbox));
        assert!(app.drawer_open);
    }

    #[test]
    fn navigate_to_snoozed_changes_view_and_nav() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::Navigate(NavTarget::View(ActiveView::Snoozed)));
        assert_eq!(app.active_view, ActiveView::Snoozed);
        assert_eq!(app.active_nav, NavTarget::View(ActiveView::Snoozed));
    }

    #[test]
    fn navigate_to_done_changes_view_and_nav() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::Navigate(NavTarget::View(ActiveView::Done)));
        assert_eq!(app.active_view, ActiveView::Done);
        assert_eq!(app.active_nav, NavTarget::View(ActiveView::Done));
    }

    #[test]
    fn navigate_to_section_does_not_change_active_view() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::Navigate(NavTarget::Section(NavSection::Drafts)));
        // active_view stays Inbox — only primary views change the toolbar
        assert_eq!(app.active_view, ActiveView::Inbox);
        assert_eq!(app.active_nav, NavTarget::Section(NavSection::Drafts));
    }

    #[test]
    fn navigate_to_bundle_category_does_not_change_active_view() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::Navigate(NavTarget::BundleCategory("Social".into())));
        assert_eq!(app.active_view, ActiveView::Inbox);
        assert_eq!(
            app.active_nav,
            NavTarget::BundleCategory("Social".into())
        );
    }

    #[test]
    fn toggle_drawer() {
        let mut app = Inboxly::default();
        assert!(app.drawer_open);
        let _ = app.update(Message::ToggleDrawer);
        assert!(!app.drawer_open);
        let _ = app.update(Message::ToggleDrawer);
        assert!(app.drawer_open);
    }

    #[test]
    fn navigate_back_to_inbox_from_done() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::Navigate(NavTarget::View(ActiveView::Done)));
        assert_eq!(app.active_view, ActiveView::Done);
        let _ = app.update(Message::Navigate(NavTarget::View(ActiveView::Inbox)));
        assert_eq!(app.active_view, ActiveView::Inbox);
        assert_eq!(app.active_nav, NavTarget::View(ActiveView::Inbox));
    }

    #[test]
    fn toolbar_color_changes_with_view() {
        let inbox_color = ActiveView::Inbox.toolbar_color();
        let snoozed_color = ActiveView::Snoozed.toolbar_color();
        let done_color = ActiveView::Done.toolbar_color();

        // Each view has a distinct colour
        assert_ne!(inbox_color, snoozed_color);
        assert_ne!(inbox_color, done_color);
        assert_ne!(snoozed_color, done_color);
    }

    #[test]
    fn view_titles() {
        assert_eq!(ActiveView::Inbox.title(), "Inbox");
        assert_eq!(ActiveView::Snoozed.title(), "Snoozed");
        assert_eq!(ActiveView::Done.title(), "Done");
    }

    #[test]
    fn nav_section_labels() {
        assert_eq!(NavSection::Drafts.label(), "Drafts");
        assert_eq!(NavSection::Sent.label(), "Sent");
        assert_eq!(NavSection::Reminders.label(), "Reminders");
        assert_eq!(NavSection::Trash.label(), "Trash");
        assert_eq!(NavSection::Spam.label(), "Spam");
    }

    #[test]
    fn default_bundle_categories_has_eight_entries() {
        let cats = crate::nav::default_bundle_categories();
        assert_eq!(cats.len(), 8);
        assert_eq!(cats[0].name, "Social");
        assert_eq!(cats[7].name, "Low Priority");
    }
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui
```

All 11 tests should pass.

**Commit:** `test(ui): add state management tests for message handling and nav logic`

---

## Task 9: Add theme colour unit tests

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme.rs` (append tests module)

Verify that the colour constants and `category_color` function return correct values. These are regression tests — if someone changes a colour constant, these will catch it.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_from_hex_black() {
        let c = color_from_hex(0, 0, 0);
        assert_eq!(c, Color::BLACK);
    }

    #[test]
    fn color_from_hex_white() {
        let c = color_from_hex(255, 255, 255);
        assert_eq!(c, Color::WHITE);
    }

    #[test]
    fn inbox_toolbar_is_blue() {
        let c = ActiveView::Inbox.toolbar_color();
        // #4285f4 → r=0x42=66, g=0x85=133, b=0xf4=244
        assert!((c.r - 66.0 / 255.0).abs() < 0.01);
        assert!((c.g - 133.0 / 255.0).abs() < 0.01);
        assert!((c.b - 244.0 / 255.0).abs() < 0.01);
    }

    #[test]
    fn snoozed_toolbar_is_orange() {
        let c = ActiveView::Snoozed.toolbar_color();
        // #ef6c00 → r=239, g=108, b=0
        assert!((c.r - 239.0 / 255.0).abs() < 0.01);
        assert!((c.g - 108.0 / 255.0).abs() < 0.01);
        assert!(c.b < 0.01);
    }

    #[test]
    fn done_toolbar_is_green() {
        let c = ActiveView::Done.toolbar_color();
        // #0f9d58 → r=15, g=157, b=88
        assert!((c.r - 15.0 / 255.0).abs() < 0.01);
        assert!((c.g - 157.0 / 255.0).abs() < 0.01);
        assert!((c.b - 88.0 / 255.0).abs() < 0.01);
    }

    #[test]
    fn social_category_color() {
        let cc = category_color("Social");
        // title: #d23f31
        assert!((cc.title.r - 0xd2 as f32 / 255.0).abs() < 0.01);
        assert!((cc.title.g - 0x3f as f32 / 255.0).abs() < 0.01);
        assert!((cc.title.b - 0x31 as f32 / 255.0).abs() < 0.01);
    }

    #[test]
    fn unknown_category_gets_default() {
        let cc = category_color("UnknownCategory");
        assert_eq!(cc.title, SECONDARY_TEXT);
    }

    #[test]
    fn layout_constants_match_spec() {
        assert_eq!(TOOLBAR_HEIGHT, 56.0);
        assert_eq!(NAV_DRAWER_WIDTH, 264.0);
        assert_eq!(NAV_ITEM_HEIGHT, 48.0);
        assert_eq!(AVATAR_DIAMETER, 40.0);
        assert_eq!(DEFAULT_PADDING, 16.0);
        assert_eq!(DIVIDER_THICKNESS, 1.0);
    }
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui
```

**Commit:** `test(ui): add theme colour and layout constant tests`

---

## Task 10: Final integration verify — build and run

No new code in this task. This is a verification step.

**Run the full build + test + clippy pipeline:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo build --workspace && cargo test --workspace && cargo clippy --workspace -- -D warnings
```

Fix any warnings or errors. Common issues to expect:
- Unused imports in `nav.rs` or `toolbar.rs` — remove them
- Iced API mismatches — the code samples above target Iced 0.13 but the API may have differences. Consult `cargo doc -p iced --open` or the iced examples at `https://github.com/iced-rs/iced/tree/master/examples` for the correct API surface.
- `Color` const expressions — Iced may not allow `Color::from_rgb()` in const position. Convert to `const fn` helpers or lazy initialization as needed.

**Then manually launch the binary:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo run -p inboxly
```

**Visual checklist:**

- [ ] Window opens at 1280x800
- [ ] Blue toolbar at the top, 56dp tall
- [ ] Hamburger button (☰) on the left of the toolbar
- [ ] "Inbox" title text in white on the toolbar
- [ ] Search input on the toolbar
- [ ] Avatar circle on the right of the toolbar
- [ ] White nav drawer on the left, 264dp wide
- [ ] Account switcher at the top of the drawer (avatar + email + "1 account")
- [ ] Divider after account switcher
- [ ] "Inbox" nav item highlighted with blue text and selected background
- [ ] "Snoozed" and "Done" below Inbox
- [ ] Divider after primary nav
- [ ] Drafts, Sent, Reminders, Trash, Spam listed
- [ ] Divider after secondary nav
- [ ] "Bundles" section header
- [ ] 8 bundle categories with coloured dots
- [ ] Clicking "Snoozed" changes toolbar to orange, title to "Snoozed", and highlights Snoozed in nav
- [ ] Clicking "Done" changes toolbar to green, title to "Done", and highlights Done in nav
- [ ] Clicking ☰ hides/shows the nav drawer
- [ ] Clicking a bundle category (e.g., "Social") highlights it in the nav
- [ ] Content area shows the placeholder text matching the current view

**Commit:** `chore(ui): fix clippy warnings and verify visual output`

---

## Summary

| Task | Description | Files | Tests |
|------|-------------|-------|-------|
| 1 | Create `inboxly-ui` crate | `inboxly-ui/Cargo.toml`, `src/lib.rs`, root `Cargo.toml` | compile check |
| 2 | ActiveView enum + colours + constants | `src/theme.rs` | Task 9 |
| 3 | Nav item types + defaults | `src/nav.rs` | Task 8 |
| 4 | Inboxly app state + Message enum + update/view | `src/app.rs` | Task 8 |
| 5 | Toolbar view | `src/toolbar.rs` | visual |
| 6 | Nav drawer view | `src/nav.rs` (append) | visual |
| 7 | Binary crate | `inboxly/Cargo.toml`, `src/main.rs`, root `Cargo.toml` | cargo run |
| 8 | State management tests | `src/app.rs` (tests mod) | 11 tests |
| 9 | Theme colour tests | `src/theme.rs` (tests mod) | 8 tests |
| 10 | Integration verify | — | build + clippy + visual |

**Total: 10 tasks, 19 unit tests, 2 new crates (`inboxly-ui`, `inboxly`), 6 new source files.**
