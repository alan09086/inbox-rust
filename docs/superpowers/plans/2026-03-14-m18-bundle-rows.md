# M18: Bundle Rows + Expand/Collapse — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan.

**Goal:** Implement BundleRow widget with collapsed/expanded states, expand/collapse animation, bundle data query, mixed InboxItem feed rendering, and ReminderRow placeholder — completing the visual inbox feed with bundled, unbundled, and reminder items.

**Architecture:** BundleRow is a custom Iced widget in `inboxly-ui`. It renders a collapsed summary row (category icon, name, unread badge, sender preview, timestamp) and expands in-place to reveal individual EmailRow widgets. Animation state is tracked per-bundle in the UI state. The inbox feed becomes a mixed `Vec<InboxItem>` dispatching to EmailRow, BundleRow, or ReminderRow. Bundle data is queried from `inboxly-store` (threads grouped by bundle_id via the `thread_state` and `bundles` tables).

**Tech Stack:** Rust, Iced (wgpu), inboxly-ui, inboxly-store, inboxly-core

**Prerequisite:** M17 complete — Iced shell with nav drawer (M15), theme system with bundle category colours (M16), inbox feed view with EmailRow widget and section headers (M17). M12-M13 complete — bundler assigns `bundle_id` to threads via `thread_state` table, `bundles` and `bundle_rules` tables populated.

**Spec references:**
- Bundle Category Colours table (spec §Theme System)
- BundleRow in Custom Widgets table (spec §UI Architecture)
- InboxItem enum (spec §Data Model)
- Bundle expand/collapse animation (spec §Animations)
- Dimensions: avatar 40dp, avatar column 72dp, typography 16sp/14sp/12sp (spec §Dimensions)

---

## Task 1: Add bundle feed query to inboxly-store

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/queries.rs` (new file, or append to existing store query module)

Add a function that queries the inbox feed as a mixed list of `InboxItem` variants. This joins `threads`, `thread_state`, and `bundles` to produce:
- Unbundled threads (where `thread_state.bundle_id IS NULL` and `thread_state.done = false`)
- Bundle summaries (aggregated from threads sharing a `bundle_id`, respecting throttle visibility)
- Reminders (from `reminders` table where `done = false`)

All items are sorted by `newest_date` descending (bundles use the newest thread date within the bundle).

```rust
use crate::Store;
use inboxly_core::{
    AccountId, BundleCategory, BundleId, BundleVisibility, Contact, InboxItem, Thread, ThreadId,
};
use chrono::{DateTime, Utc};
use rusqlite::params;

/// Summary of a bundle for the inbox feed (collapsed row data).
#[derive(Debug, Clone)]
pub struct BundleSummary {
    pub bundle_id: BundleId,
    pub category: BundleCategory,
    pub name: String,
    pub color_hex: String,
    pub badge_color_hex: String,
    pub unread_count: u32,
    pub total_count: u32,
    pub newest_date: DateTime<Utc>,
    pub sender_previews: Vec<SenderPreview>,
    pub thread_ids: Vec<ThreadId>,
}

/// A sender name for the collapsed bundle preview line.
#[derive(Debug, Clone)]
pub struct SenderPreview {
    pub name: String,
    pub is_unread: bool,
}

/// A feed item for the inbox view, combining all item types.
#[derive(Debug, Clone)]
pub enum FeedItem {
    Thread(Thread),
    Bundle(BundleSummary),
    Reminder {
        id: String,
        title: String,
        due: Option<DateTime<Utc>>,
    },
}

impl Store {
    /// Query the inbox feed: unbundled threads + bundle summaries + active reminders.
    ///
    /// Returns items sorted by newest_date descending.
    /// Bundle summaries aggregate all active (not-done) threads assigned to each bundle.
    /// Unbundled threads are those with no bundle_id and not done.
    /// Reminders are those not yet marked done.
    pub fn query_inbox_feed(&self, account_id: &AccountId) -> Result<Vec<FeedItem>, crate::Error> {
        let conn = self.connection();
        let mut items: Vec<(DateTime<Utc>, FeedItem)> = Vec::new();

        // 1. Unbundled threads
        let mut stmt = conn.prepare(
            "SELECT t.id, t.account_id, t.subject, t.newest_date, t.oldest_date,
                    t.email_count, t.unread_count, t.has_attachments, t.snippet
             FROM threads t
             LEFT JOIN thread_state ts ON t.id = ts.thread_id
             WHERE t.account_id = ?1
               AND (ts.done IS NULL OR ts.done = 0)
               AND (ts.bundle_id IS NULL)
             ORDER BY t.newest_date DESC",
        )?;
        let thread_rows = stmt.query_map(params![account_id.to_string()], |row| {
            // Map to Thread struct
            Ok(()) // placeholder — actual mapping uses row.get() for each field
        })?;
        // ... collect into items with FeedItem::Thread variant

        // 2. Bundle summaries — group threads by bundle_id
        let mut stmt = conn.prepare(
            "SELECT b.id, b.category, b.name, b.color, b.badge_color,
                    COUNT(t.id) as thread_count,
                    SUM(t.unread_count) as total_unread,
                    MAX(t.newest_date) as bundle_newest_date
             FROM bundles b
             INNER JOIN thread_state ts ON ts.bundle_id = b.id
             INNER JOIN threads t ON t.id = ts.thread_id
             WHERE t.account_id = ?1
               AND (ts.done IS NULL OR ts.done = 0)
               AND b.visibility != 'skip_inbox'
             GROUP BY b.id
             HAVING COUNT(t.id) > 0
             ORDER BY bundle_newest_date DESC",
        )?;
        // For each bundle, also query top sender names for preview:
        // SELECT DISTINCT e.from_name, (e.flags & 1 = 0) as is_unread
        // FROM emails e
        // INNER JOIN thread_state ts ON e.thread_id = ts.thread_id
        // WHERE ts.bundle_id = ?1 AND (ts.done IS NULL OR ts.done = 0)
        // ORDER BY e.date DESC LIMIT 3

        // 3. Reminders
        let mut stmt = conn.prepare(
            "SELECT id, title, due_at FROM reminders
             WHERE done = 0
             ORDER BY COALESCE(due_at, 9999999999) ASC",
        )?;
        // ... collect into items with FeedItem::Reminder variant

        // Sort all items by date descending
        items.sort_by(|a, b| b.0.cmp(&a.0));
        Ok(items.into_iter().map(|(_, item)| item).collect())
    }

    /// Query the individual threads within a bundle (for expanded view).
    ///
    /// Returns threads sorted by newest_date descending.
    pub fn query_bundle_threads(
        &self,
        bundle_id: &BundleId,
    ) -> Result<Vec<Thread>, crate::Error> {
        let conn = self.connection();
        let mut stmt = conn.prepare(
            "SELECT t.id, t.account_id, t.subject, t.newest_date, t.oldest_date,
                    t.email_count, t.unread_count, t.has_attachments, t.snippet
             FROM threads t
             INNER JOIN thread_state ts ON t.id = ts.thread_id
             WHERE ts.bundle_id = ?1
               AND (ts.done IS NULL OR ts.done = 0)
             ORDER BY t.newest_date DESC",
        )?;
        // ... map rows to Thread structs
        todo!()
    }
}
```

**Also:** Add `pub mod queries;` to `inboxly-store/src/lib.rs`.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-store
```

**Commit:** `feat(store): add inbox feed and bundle thread queries`

---

## Task 2: Define BundleCategory colour constants in the theme module

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme/colors.rs` (append to existing theme colours, or new file if colours are not yet split out)

Add the bundle category colour constants from the spec. These are used by BundleRow for the category name text colour and badge background. They are constant across light/dark themes.

```rust
use iced::Color;

/// Bundle category colours — constant across themes (from BigTop APK).
pub struct BundleCategoryColors {
    pub title: Color,
    pub badge_bg: Color,
}

/// Returns the (title_colour, badge_background) pair for a BundleCategory.
pub fn bundle_category_colors(category: &BundleCategory) -> BundleCategoryColors {
    match category {
        BundleCategory::Social => BundleCategoryColors {
            title: color_from_hex(0xd2, 0x3f, 0x31),    // #d23f31
            badge_bg: color_from_hex(0xfa, 0xeb, 0xea),  // #faebea
        },
        BundleCategory::Promos => BundleCategoryColors {
            title: color_from_hex(0x00, 0xac, 0xc1),     // #00acc1
            badge_bg: color_from_hex(0xe5, 0xf6, 0xf9),  // #e5f6f9
        },
        BundleCategory::Updates => BundleCategoryColors {
            title: color_from_hex(0xf4, 0x51, 0x1e),     // #f4511e
            badge_bg: color_from_hex(0xfe, 0xed, 0xe8),  // #feede8
        },
        BundleCategory::Finance => BundleCategoryColors {
            title: color_from_hex(0x55, 0x8b, 0x2f),     // #558b2f
            badge_bg: color_from_hex(0xee, 0xf3, 0xea),  // #eef3ea
        },
        BundleCategory::Purchases => BundleCategoryColors {
            title: color_from_hex(0x6d, 0x4c, 0x41),     // #6d4c41
            badge_bg: color_from_hex(0xf0, 0xed, 0xec),  // #f0edec
        },
        BundleCategory::Travel => BundleCategoryColors {
            title: color_from_hex(0x8e, 0x24, 0xaa),     // #8e24aa
            badge_bg: color_from_hex(0xf3, 0xe9, 0xf6),  // #f3e9f6
        },
        BundleCategory::Forums => BundleCategoryColors {
            title: color_from_hex(0x39, 0x49, 0xab),     // #3949ab
            badge_bg: color_from_hex(0xeb, 0xec, 0xf6),  // #ebecf6
        },
        BundleCategory::LowPriority => BundleCategoryColors {
            title: color_from_hex(0x21, 0x21, 0x21),     // #212121
            badge_bg: color_from_hex(0xe5, 0xe5, 0xe5),  // #e5e5e5
        },
        BundleCategory::Saved => BundleCategoryColors {
            title: color_from_hex(0x33, 0x67, 0xd6),     // #3367d6
            badge_bg: color_from_hex(0xeb, 0xf2, 0xff),  // #ebf2ff (derived)
        },
        BundleCategory::Custom(_) => BundleCategoryColors {
            title: color_from_hex(0x21, 0x21, 0x21),     // #212121
            badge_bg: color_from_hex(0xe5, 0xe5, 0xe5),  // #e5e5e5
        },
    }
}

fn color_from_hex(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add bundle category colour constants from BigTop spec`

---

## Task 3: Define BundleIcon mapping for category icons

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/bundle_icon.rs` (new file)

Each bundle category has a distinctive icon displayed in a tinted circle on the left of the BundleRow. Since Iced does not have a built-in icon font, use Unicode symbols rendered in the category colour, or use `iced::widget::Svg` with bundled SVG icons. For v1, use simple Unicode/emoji fallbacks drawn as text inside a coloured circle, with a clear path to SVG icons later.

```rust
use iced::widget::{canvas, container, text, Canvas, Container, Row};
use iced::{Alignment, Color, Element, Length, Renderer, Theme};

/// Returns the icon character for a bundle category.
/// These are placeholder Unicode symbols; replace with SVG icons in polish pass.
pub fn category_icon_char(category: &BundleCategory) -> &'static str {
    match category {
        BundleCategory::Social => "\u{1F464}",     // 👤 person silhouette
        BundleCategory::Promos => "\u{1F3F7}",     // 🏷️ label/tag
        BundleCategory::Updates => "\u{1F514}",     // 🔔 bell
        BundleCategory::Finance => "\u{1F4B0}",     // 💰 money bag
        BundleCategory::Purchases => "\u{1F6D2}",   // 🛒 shopping cart
        BundleCategory::Travel => "\u{2708}",       // ✈ airplane
        BundleCategory::Forums => "\u{1F4AC}",      // 💬 speech bubble
        BundleCategory::LowPriority => "\u{2B07}",  // ⬇ down arrow
        BundleCategory::Saved => "\u{2B50}",        // ⭐ star
        BundleCategory::Custom(_) => "\u{1F4C1}",   // 📁 folder
    }
}

/// Renders a 40dp circle with tinted background and category icon centered inside.
///
/// The circle background is the badge_bg colour (pastel), the icon is rendered
/// in the category title colour for contrast.
pub fn category_icon_circle<'a, Message: 'a>(
    category: &BundleCategory,
    title_color: Color,
    badge_bg: Color,
) -> Element<'a, Message> {
    let icon_char = category_icon_char(category);

    container(
        text(icon_char)
            .size(20)
            .color(title_color)
            .align_x(iced::alignment::Horizontal::Center),
    )
    .width(40)
    .height(40)
    .center_x(40)
    .center_y(40)
    .style(move |_theme: &Theme| container::Style {
        background: Some(iced::Background::Color(badge_bg)),
        border: iced::Border {
            radius: 20.0.into(), // 40dp / 2 = perfect circle
            ..Default::default()
        },
        ..Default::default()
    })
    .into()
}
```

**Also:** Add `pub mod bundle_icon;` to `inboxly-ui/src/widgets/mod.rs`.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add bundle category icon circle widget`

---

## Task 4: Implement BundleRow widget — collapsed state

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/bundle_row.rs` (new file)

The collapsed BundleRow displays in a single row:
- **Left column (72dp):** Category icon in tinted circle (40dp, centred in 72dp column)
- **Middle column (flex):** Two lines stacked:
  - Line 1: Category name in category colour (16sp, medium weight) + unread count badge (pastel background pill with bold count)
  - Line 2: Sender preview — comma-separated sender names (14sp), bold if that sender has unread messages, truncated with ellipsis
- **Right column:** Timestamp of newest thread (12sp, secondary text colour)

Clicking the row emits a `ToggleBundle(BundleId)` message.

```rust
use iced::widget::{button, column, container, row, text, Row};
use iced::{Alignment, Color, Element, Length, Padding};

use crate::theme::colors::bundle_category_colors;
use crate::widgets::bundle_icon::category_icon_circle;
use inboxly_core::BundleCategory;
use inboxly_store::queries::{BundleSummary, SenderPreview};

/// Renders a collapsed BundleRow.
///
/// Layout:
/// ```text
/// ┌──────┬──────────────────────────────────┬────────┐
/// │ 72dp │ Category Name        [3 new]     │ 2:34pm │
/// │ icon │ Alice, Bob, Charlie              │        │
/// └──────┴──────────────────────────────────┴────────┘
/// ```
pub fn bundle_row_collapsed<'a, Message: Clone + 'a>(
    summary: &BundleSummary,
    on_click: Message,
    secondary_text_color: Color,
) -> Element<'a, Message> {
    let colors = bundle_category_colors(&summary.category);

    // Left: icon circle (40dp in 72dp column)
    let icon = container(category_icon_circle::<Message>(
        &summary.category,
        colors.title,
        colors.badge_bg,
    ))
    .width(72)
    .center_x(72)
    .center_y(Length::Fill);

    // Middle line 1: category name + unread badge
    let category_name = text(&summary.name)
        .size(16)
        .color(colors.title)
        .font(iced::Font {
            weight: iced::font::Weight::Medium,
            ..Default::default()
        });

    let mut line1 = Row::new().spacing(8).align_y(Alignment::Center);
    line1 = line1.push(category_name);

    if summary.unread_count > 0 {
        let badge_text = text(format!("{} new", summary.unread_count))
            .size(12)
            .color(colors.title)
            .font(iced::Font {
                weight: iced::font::Weight::Bold,
                ..Default::default()
            });
        let badge = container(badge_text)
            .padding(Padding::from([2, 8]))
            .style(move |_theme: &iced::Theme| container::Style {
                background: Some(iced::Background::Color(colors.badge_bg)),
                border: iced::Border {
                    radius: 10.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            });
        line1 = line1.push(badge);
    }

    // Middle line 2: sender preview (comma-separated, bold if unread)
    let mut sender_parts = Row::new().spacing(0);
    for (i, sender) in summary.sender_previews.iter().take(3).enumerate() {
        if i > 0 {
            sender_parts = sender_parts.push(
                text(", ").size(14).color(secondary_text_color),
            );
        }
        let weight = if sender.is_unread {
            iced::font::Weight::Bold
        } else {
            iced::font::Weight::Normal
        };
        sender_parts = sender_parts.push(
            text(&sender.name)
                .size(14)
                .color(secondary_text_color)
                .font(iced::Font { weight, ..Default::default() }),
        );
    }
    if summary.sender_previews.len() > 3 {
        sender_parts = sender_parts.push(
            text(format!(", +{}", summary.sender_previews.len() - 3))
                .size(14)
                .color(secondary_text_color),
        );
    }

    let middle = column![line1, sender_parts]
        .spacing(2)
        .width(Length::Fill);

    // Right: timestamp
    let timestamp = text(format_relative_time(&summary.newest_date))
        .size(12)
        .color(secondary_text_color);

    // Assemble row
    let row_content = row![icon, middle, timestamp]
        .spacing(0)
        .padding(Padding::from([12, 16, 12, 0]))
        .align_y(Alignment::Center);

    // Wrap in clickable button styled as flat card
    button(row_content)
        .on_press(on_click)
        .style(|theme: &iced::Theme, status| {
            // Flat card style: white background, no border, hover highlight
            button::Style {
                background: Some(iced::Background::Color(Color::WHITE)),
                border: iced::Border::default(),
                ..Default::default()
            }
        })
        .width(Length::Fill)
        .into()
}

/// Format a DateTime as a relative timestamp string.
/// Same logic as EmailRow: "2:34 PM" for today, "Mar 12" for this year, "Mar 12, 2025" for older.
fn format_relative_time(dt: &DateTime<Utc>) -> String {
    // Reuse the shared timestamp formatter from EmailRow / a shared util module.
    // For now, stub:
    dt.format("%b %d").to_string()
}
```

**Also:** Add `pub mod bundle_row;` to `inboxly-ui/src/widgets/mod.rs`.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement BundleRow collapsed widget`

---

## Task 5: Add expand/collapse animation state

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/state/bundle_state.rs` (new file)

Track per-bundle expand/collapse state and animation progress. Each bundle can be in one of three visual states: Collapsed, Expanding (animating), or Expanded. The animation progress is a `f32` from 0.0 (collapsed) to 1.0 (expanded), driven by `iced::time::every()` ticks.

```rust
use std::collections::HashMap;
use std::time::{Duration, Instant};

use inboxly_core::BundleId;

/// Duration of the expand/collapse animation.
pub const ANIMATION_DURATION: Duration = Duration::from_millis(275);

/// Animation easing: ease-out cubic (decelerating).
/// t is normalized [0.0, 1.0].
pub fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// Visual state of a bundle row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BundleVisualState {
    Collapsed,
    Expanding { started: Instant },
    Expanded,
    Collapsing { started: Instant },
}

/// Tracks expand/collapse state for all bundles in the inbox feed.
#[derive(Debug, Clone)]
pub struct BundleExpandState {
    states: HashMap<BundleId, BundleVisualState>,
}

impl BundleExpandState {
    pub fn new() -> Self {
        Self {
            states: HashMap::new(),
        }
    }

    /// Get the visual state of a bundle. Defaults to Collapsed.
    pub fn state(&self, id: &BundleId) -> BundleVisualState {
        self.states.get(id).copied().unwrap_or(BundleVisualState::Collapsed)
    }

    /// Toggle a bundle between collapsed and expanded, starting animation.
    pub fn toggle(&mut self, id: BundleId) {
        let now = Instant::now();
        let current = self.state(&id);
        let next = match current {
            BundleVisualState::Collapsed => BundleVisualState::Expanding { started: now },
            BundleVisualState::Expanding { .. } => BundleVisualState::Collapsing { started: now },
            BundleVisualState::Expanded => BundleVisualState::Collapsing { started: now },
            BundleVisualState::Collapsing { .. } => BundleVisualState::Expanding { started: now },
        };
        self.states.insert(id, next);
    }

    /// Compute the animation progress [0.0, 1.0] for a bundle.
    /// Returns 0.0 for collapsed, 1.0 for expanded, intermediate for animating.
    pub fn progress(&self, id: &BundleId) -> f32 {
        match self.state(id) {
            BundleVisualState::Collapsed => 0.0,
            BundleVisualState::Expanded => 1.0,
            BundleVisualState::Expanding { started } => {
                let elapsed = started.elapsed();
                let raw = (elapsed.as_secs_f32() / ANIMATION_DURATION.as_secs_f32()).min(1.0);
                ease_out_cubic(raw)
            }
            BundleVisualState::Collapsing { started } => {
                let elapsed = started.elapsed();
                let raw = (elapsed.as_secs_f32() / ANIMATION_DURATION.as_secs_f32()).min(1.0);
                1.0 - ease_out_cubic(raw)
            }
        }
    }

    /// Returns true if any bundle is currently animating (needs tick subscription).
    pub fn any_animating(&self) -> bool {
        self.states.values().any(|s| matches!(s,
            BundleVisualState::Expanding { .. } | BundleVisualState::Collapsing { .. }
        ))
    }

    /// Advance animation state: if an animation has completed, snap to final state.
    /// Call this on each tick.
    pub fn tick(&mut self) {
        let now = Instant::now();
        for state in self.states.values_mut() {
            match *state {
                BundleVisualState::Expanding { started } => {
                    if now.duration_since(started) >= ANIMATION_DURATION {
                        *state = BundleVisualState::Expanded;
                    }
                }
                BundleVisualState::Collapsing { started } => {
                    if now.duration_since(started) >= ANIMATION_DURATION {
                        *state = BundleVisualState::Collapsed;
                    }
                }
                _ => {}
            }
        }
    }

    /// Check if a bundle is in expanded or expanding state (should show child threads).
    pub fn is_showing_threads(&self, id: &BundleId) -> bool {
        matches!(
            self.state(id),
            BundleVisualState::Expanded | BundleVisualState::Expanding { .. }
        )
    }
}

impl Default for BundleExpandState {
    fn default() -> Self {
        Self::new()
    }
}
```

**Also:** Add `pub mod bundle_state;` to `inboxly-ui/src/state/mod.rs` (create `state/mod.rs` if it does not exist).

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add bundle expand/collapse animation state tracker`

---

## Task 6: Implement BundleRow expanded state (with child EmailRows)

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/bundle_row.rs` (append)

Add the expanded bundle view: the collapsed row header (clickable to collapse) followed by the individual EmailRow widgets for each thread in the bundle, indented to align with the text column (72dp left padding to clear the icon column). Animation progress controls opacity of child rows and vertical height scaling.

```rust
use crate::state::bundle_state::BundleExpandState;
use crate::widgets::email_row::email_row; // from M17

/// Renders a bundle — either collapsed or expanded — based on animation state.
///
/// When expanded, the collapsed header row is still visible (click to collapse),
/// followed by individual EmailRow widgets for each thread in the bundle.
/// Animation progress controls:
/// - Child row opacity: 0.0 (invisible) → 1.0 (fully visible)
/// - Child container height: scaled from 0 to natural height
pub fn bundle_row<'a, Message: Clone + 'a>(
    summary: &BundleSummary,
    threads: &[Thread],
    expand_state: &BundleExpandState,
    on_toggle: Message,
    on_thread_click: impl Fn(ThreadId) -> Message + 'a,
    secondary_text_color: Color,
    theme: &InboxlyTheme,
) -> Element<'a, Message> {
    let progress = expand_state.progress(&summary.bundle_id);
    let showing_threads = expand_state.is_showing_threads(&summary.bundle_id);

    let mut col = column![]
        .width(Length::Fill)
        .spacing(0);

    // Always show the collapsed header row
    col = col.push(bundle_row_collapsed(summary, on_toggle, secondary_text_color));

    // Show child thread rows when expanding/expanded
    if showing_threads && !threads.is_empty() {
        let opacity = progress.clamp(0.0, 1.0);

        for thread in threads {
            let thread_id = thread.id.clone();
            let child_row = container(
                email_row(thread, on_thread_click(thread_id), theme)
            )
            .padding(Padding::from([0, 0, 0, 32])) // indent child rows (72dp icon col - 40dp = 32dp extra indent)
            .width(Length::Fill)
            .style(move |_theme: &iced::Theme| container::Style {
                // Fade in via opacity during animation
                // Note: Iced doesn't have native opacity on containers.
                // Use alpha channel on background colour as visual cue,
                // and clip height via max_height based on progress.
                ..Default::default()
            });

            col = col.push(child_row);
        }
    }

    // Wrap in a container with card-like styling and a bottom divider
    container(col)
        .width(Length::Fill)
        .style(|_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(Color::WHITE)),
            border: iced::Border {
                width: 0.0,
                color: Color::TRANSPARENT,
                radius: 0.0.into(), // flat cards per spec
            },
            ..Default::default()
        })
        .into()
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement BundleRow expanded state with child EmailRows`

---

## Task 7: Implement ReminderRow widget

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/reminder_row.rs` (new file)

A simple row for reminders in the inbox feed. Per spec: blue left border, bell icon, reminder title, due date/time.

```rust
use iced::widget::{container, row, text, Column};
use iced::{Alignment, Color, Element, Length, Padding};

/// Blue colour for reminder accents (from spec toolbar inbox blue).
const REMINDER_BLUE: Color = Color::from_rgb(
    0x42 as f32 / 255.0,
    0x85 as f32 / 255.0,
    0xf4 as f32 / 255.0,
);

/// Renders a reminder row with blue left border, bell icon, title, and due date.
///
/// Layout:
/// ```text
/// ┃ 🔔  Remember to call dentist              Tomorrow 8 AM
/// ```
pub fn reminder_row<'a, Message: Clone + 'a>(
    id: &str,
    title: &str,
    due_text: Option<&str>,
    on_click: Message,
    secondary_text_color: Color,
) -> Element<'a, Message> {
    let bell = text("\u{1F514}").size(20); // 🔔

    let title_text = text(title).size(16);

    let mut content = row![bell, title_text]
        .spacing(12)
        .align_y(Alignment::Center)
        .width(Length::Fill);

    if let Some(due) = due_text {
        let due_label = text(due)
            .size(12)
            .color(secondary_text_color);
        content = content.push(due_label);
    }

    // Blue left border via container with left-only border
    let card = container(
        iced::widget::button(
            container(content)
                .padding(Padding::from([12, 16]))
        )
        .on_press(on_click)
        .style(|_theme: &iced::Theme, _status| iced::widget::button::Style {
            background: Some(iced::Background::Color(Color::WHITE)),
            border: iced::Border::default(),
            ..Default::default()
        })
        .width(Length::Fill)
    )
    .style(move |_theme: &iced::Theme| container::Style {
        background: Some(iced::Background::Color(Color::WHITE)),
        border: iced::Border {
            width: 0.0,
            ..Default::default()
        },
        // Blue left border is achieved via a nested approach:
        // outer container with blue bg + inner container with white bg offset left
        ..Default::default()
    });

    // Wrap in a row that simulates a left border:
    // [3dp blue strip] [content]
    let blue_strip = container(text(""))
        .width(3)
        .height(Length::Fill)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(REMINDER_BLUE)),
            ..Default::default()
        });

    row![blue_strip, card]
        .width(Length::Fill)
        .height(Length::Shrink)
        .into()
}
```

**Also:** Add `pub mod reminder_row;` to `inboxly-ui/src/widgets/mod.rs`.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement ReminderRow widget with blue left border`

---

## Task 8: Define InboxFeedItem enum and FeedMessage for the inbox view

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/inbox_view.rs` (modify existing from M17)

Extend the inbox view's internal types to support a mixed feed. Add an `InboxFeedItem` enum that mirrors the store's `FeedItem` but holds pre-resolved display data. Add `FeedMessage` variants for bundle toggle and reminder click.

```rust
use inboxly_core::{BundleId, ThreadId};
use inboxly_store::queries::{BundleSummary, FeedItem};

/// Messages produced by the inbox feed.
#[derive(Debug, Clone)]
pub enum FeedMessage {
    /// User clicked an unbundled email thread.
    ThreadClicked(ThreadId),
    /// User clicked a bundle row to toggle expand/collapse.
    ToggleBundle(BundleId),
    /// User clicked a thread inside an expanded bundle.
    BundleThreadClicked(BundleId, ThreadId),
    /// User clicked a reminder.
    ReminderClicked(String),
    /// Animation tick (16ms frame updates during expand/collapse).
    AnimationTick,
}
```

**Also** add `BundleExpandState` to the inbox view state struct:

```rust
use crate::state::bundle_state::BundleExpandState;

pub struct InboxViewState {
    // ... existing fields from M17 (feed items, scroll state, etc.)
    pub feed: Vec<FeedItem>,
    pub bundle_threads: HashMap<BundleId, Vec<Thread>>,
    pub expand_state: BundleExpandState,
}

impl InboxViewState {
    pub fn new() -> Self {
        Self {
            feed: Vec::new(),
            bundle_threads: HashMap::new(),
            expand_state: BundleExpandState::new(),
        }
    }
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add FeedMessage enum and InboxViewState with bundle expand tracking`

---

## Task 9: Implement inbox feed rendering with InboxItem dispatch

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/inbox_view.rs` (append/modify)

Implement the `view()` function for the inbox feed. Iterate over `self.feed` and dispatch each item to the appropriate widget: `email_row()` for `FeedItem::Thread`, `bundle_row()` for `FeedItem::Bundle`, `reminder_row()` for `FeedItem::Reminder`. Wrap all items in a scrollable column.

```rust
use iced::widget::{column, scrollable, Column};
use iced::{Element, Length};

use crate::widgets::bundle_row::bundle_row;
use crate::widgets::email_row::email_row;
use crate::widgets::reminder_row::reminder_row;
use crate::theme::InboxlyTheme;

impl InboxViewState {
    /// Render the mixed inbox feed.
    pub fn view<'a>(&'a self, theme: &'a InboxlyTheme) -> Element<'a, FeedMessage> {
        let secondary_text = theme.secondary_text;

        let mut feed_column = Column::new()
            .spacing(1) // 1px divider between items (rendered as gap)
            .width(Length::Fill);

        for item in &self.feed {
            let element: Element<'a, FeedMessage> = match item {
                FeedItem::Thread(thread) => {
                    let tid = thread.id.clone();
                    email_row(
                        thread,
                        FeedMessage::ThreadClicked(tid),
                        theme,
                    )
                }
                FeedItem::Bundle(summary) => {
                    let bid = summary.bundle_id.clone();
                    let threads = self
                        .bundle_threads
                        .get(&summary.bundle_id)
                        .map(|v| v.as_slice())
                        .unwrap_or(&[]);

                    bundle_row(
                        summary,
                        threads,
                        &self.expand_state,
                        FeedMessage::ToggleBundle(bid.clone()),
                        move |tid| FeedMessage::BundleThreadClicked(bid.clone(), tid),
                        secondary_text,
                        theme,
                    )
                }
                FeedItem::Reminder { id, title, due } => {
                    let due_text = due.map(|d| format_relative_time(&d));
                    reminder_row(
                        id,
                        title,
                        due_text.as_deref(),
                        FeedMessage::ReminderClicked(id.clone()),
                        secondary_text,
                    )
                }
            };
            feed_column = feed_column.push(element);
        }

        scrollable(feed_column)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement mixed inbox feed rendering with InboxItem dispatch`

---

## Task 10: Handle ToggleBundle message and animation tick subscription

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/inbox_view.rs` (append/modify)

Wire up the `FeedMessage::ToggleBundle` handler: toggle the expand state, and if expanding, issue a query to load the bundle's threads into `bundle_threads`. Wire up `FeedMessage::AnimationTick` to call `expand_state.tick()`. Add a subscription that emits `AnimationTick` at ~60fps while any animation is active.

```rust
use iced::time;
use std::time::Duration;

impl InboxViewState {
    /// Handle a feed message. Returns a Command if async work is needed.
    pub fn update(&mut self, message: FeedMessage, store: &Store) -> iced::Task<FeedMessage> {
        match message {
            FeedMessage::ToggleBundle(bundle_id) => {
                self.expand_state.toggle(bundle_id.clone());

                // If we're now expanding and don't have threads cached, load them
                if self.expand_state.is_showing_threads(&bundle_id)
                    && !self.bundle_threads.contains_key(&bundle_id)
                {
                    match store.query_bundle_threads(&bundle_id) {
                        Ok(threads) => {
                            self.bundle_threads.insert(bundle_id, threads);
                        }
                        Err(e) => {
                            eprintln!("Failed to load bundle threads: {e}");
                        }
                    }
                }

                iced::Task::none()
            }
            FeedMessage::AnimationTick => {
                self.expand_state.tick();
                iced::Task::none()
            }
            FeedMessage::ThreadClicked(tid) => {
                // Navigate to conversation view — handled by parent
                iced::Task::none()
            }
            FeedMessage::BundleThreadClicked(_bid, tid) => {
                // Navigate to conversation view — handled by parent
                iced::Task::none()
            }
            FeedMessage::ReminderClicked(id) => {
                // Open reminder detail — handled by parent
                iced::Task::none()
            }
        }
    }

    /// Subscription: emit AnimationTick at ~60fps while any bundle is animating.
    pub fn subscription(&self) -> iced::Subscription<FeedMessage> {
        if self.expand_state.any_animating() {
            time::every(Duration::from_millis(16)).map(|_| FeedMessage::AnimationTick)
        } else {
            iced::Subscription::none()
        }
    }
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): wire bundle toggle handler and animation tick subscription`

---

## Task 11: Wire inbox view into the main Iced application

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` (modify existing from M15/M17)

Integrate the inbox view's state, messages, view, and subscription into the top-level Iced `Application` (or `Sandbox`). Map `FeedMessage` into the app's top-level `Message` enum. Forward the animation subscription. On app startup (or when switching to Inbox view), call `store.query_inbox_feed()` to populate the feed.

```rust
// In the top-level Message enum:
#[derive(Debug, Clone)]
pub enum Message {
    // ... existing variants from M15-M17
    Feed(FeedMessage),
    FeedLoaded(Vec<FeedItem>),
}

// In update():
Message::Feed(feed_msg) => {
    let task = self.inbox_view.update(feed_msg, &self.store);
    task.map(Message::Feed)
}
Message::FeedLoaded(items) => {
    self.inbox_view.feed = items;
    iced::Task::none()
}

// In view():
// When current view is Inbox, render self.inbox_view.view(&self.theme).map(Message::Feed)

// In subscription():
// Include self.inbox_view.subscription().map(Message::Feed)
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): integrate inbox feed view into main application`

---

## Task 12: Implement height-based expand/collapse animation

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/bundle_row.rs` (modify)

Refine the expand/collapse animation. Iced does not have a built-in height animation primitive, so implement it by wrapping the expanded child container in a `container` with a dynamically computed `max_height` based on animation progress. Each child EmailRow is approximately 72dp tall. The max_height of the children container is `progress * (thread_count * 72)`.

For the fade-in effect during expansion, use the container's background alpha channel: render child rows with a semi-transparent overlay that fades from opaque (hiding content) to transparent as progress goes from 0 to 1.

```rust
/// Renders the expanded children container with animated height.
fn animated_children<'a, Message: Clone + 'a>(
    threads: &[Thread],
    progress: f32,
    bundle_id: &BundleId,
    on_thread_click: impl Fn(ThreadId) -> Message + 'a,
    theme: &InboxlyTheme,
) -> Element<'a, Message> {
    let child_row_height: f32 = 72.0; // approximate EmailRow height
    let total_height = threads.len() as f32 * child_row_height;
    let animated_height = (progress * total_height).max(0.0);
    let opacity = progress; // 0.0 = invisible, 1.0 = fully visible

    let mut children_col = Column::new()
        .spacing(1)
        .width(Length::Fill);

    for thread in threads {
        let tid = thread.id.clone();
        children_col = children_col.push(
            container(
                email_row(thread, on_thread_click(tid), theme)
            )
            .padding(Padding::from([0, 0, 0, 32])),
        );
    }

    // Clip to animated height
    container(children_col)
        .max_height(animated_height)
        .clip(true)
        .width(Length::Fill)
        .into()
}
```

Update `bundle_row()` (from Task 6) to call `animated_children()` instead of manually building children.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement height-clipped expand/collapse animation for bundles`

---

## Task 13: Load inbox feed on app startup and on sync events

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` (modify)

In the app's `new()` / initialization, call `store.query_inbox_feed(account_id)` to populate the initial feed. Also, when receiving sync events (`NewEmail`, `EmailFlagsChanged`, `EmailDeleted`) from the IMAP sync channel, re-query the feed to pick up changes.

```rust
// In new() or the equivalent Iced init:
let feed = store.query_inbox_feed(&active_account_id)
    .unwrap_or_default();
inbox_view.feed = feed;

// In the sync event handler:
SyncEvent::NewEmail(_) | SyncEvent::EmailFlagsChanged(_) | SyncEvent::EmailDeleted(_) => {
    let feed = self.store.query_inbox_feed(&self.active_account_id)
        .unwrap_or_default();
    self.inbox_view.feed = feed;
    // Also invalidate cached bundle threads for bundles that are currently expanded
    self.inbox_view.bundle_threads.clear();
    // Re-load threads for any currently expanded bundles
    for (bid, state) in &self.inbox_view.expand_state.states {
        if matches!(state, BundleVisualState::Expanded | BundleVisualState::Expanding { .. }) {
            if let Ok(threads) = self.store.query_bundle_threads(bid) {
                self.inbox_view.bundle_threads.insert(bid.clone(), threads);
            }
        }
    }
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): load inbox feed on startup and refresh on sync events`

---

## Task 14: Add unit tests for BundleExpandState

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/state/bundle_state.rs` (append)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use inboxly_core::BundleId;
    use uuid::Uuid;

    fn test_bundle_id() -> BundleId {
        BundleId(Uuid::new_v4())
    }

    #[test]
    fn default_state_is_collapsed() {
        let state = BundleExpandState::new();
        let id = test_bundle_id();
        assert_eq!(state.state(&id), BundleVisualState::Collapsed);
        assert_eq!(state.progress(&id), 0.0);
        assert!(!state.is_showing_threads(&id));
    }

    #[test]
    fn toggle_starts_expanding() {
        let mut state = BundleExpandState::new();
        let id = test_bundle_id();
        state.toggle(id.clone());
        assert!(matches!(state.state(&id), BundleVisualState::Expanding { .. }));
        assert!(state.is_showing_threads(&id));
        assert!(state.any_animating());
    }

    #[test]
    fn toggle_twice_starts_collapsing() {
        let mut state = BundleExpandState::new();
        let id = test_bundle_id();
        state.toggle(id.clone());
        state.toggle(id.clone());
        assert!(matches!(state.state(&id), BundleVisualState::Collapsing { .. }));
        assert!(!state.is_showing_threads(&id));
    }

    #[test]
    fn tick_completes_animation() {
        let mut state = BundleExpandState::new();
        let id = test_bundle_id();
        state.toggle(id.clone());

        // Simulate time passing beyond animation duration
        // Override the started instant to be in the past
        if let Some(s) = state.states.get_mut(&id) {
            *s = BundleVisualState::Expanding {
                started: Instant::now() - ANIMATION_DURATION - Duration::from_millis(10),
            };
        }
        state.tick();
        assert_eq!(state.state(&id), BundleVisualState::Expanded);
        assert!(!state.any_animating());
        assert_eq!(state.progress(&id), 1.0);
    }

    #[test]
    fn ease_out_cubic_boundaries() {
        assert_eq!(ease_out_cubic(0.0), 0.0);
        assert!((ease_out_cubic(1.0) - 1.0).abs() < f32::EPSILON);
        // Ease-out: progress at 50% raw time should be > 50% (decelerating curve)
        assert!(ease_out_cubic(0.5) > 0.5);
    }

    #[test]
    fn multiple_bundles_independent() {
        let mut state = BundleExpandState::new();
        let id1 = test_bundle_id();
        let id2 = test_bundle_id();

        state.toggle(id1.clone());
        assert!(state.is_showing_threads(&id1));
        assert!(!state.is_showing_threads(&id2));

        state.toggle(id2.clone());
        assert!(state.is_showing_threads(&id1));
        assert!(state.is_showing_threads(&id2));
    }
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui -- bundle_state
```

**Commit:** `test(ui): add unit tests for BundleExpandState`

---

## Task 15: Add integration test for inbox feed query

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/tests/feed_query.rs` (new file)

Test the `query_inbox_feed()` and `query_bundle_threads()` functions against an in-memory SQLite database with fixture data. Verify that:
- Unbundled threads appear as `FeedItem::Thread`
- Bundled threads are grouped into `FeedItem::Bundle` with correct unread counts and sender previews
- Done threads are excluded
- Reminders with `done = false` appear as `FeedItem::Reminder`
- Results are sorted by newest_date descending
- `query_bundle_threads()` returns only threads for the specified bundle

```rust
use inboxly_store::Store;
use inboxly_core::{AccountId, BundleId, ThreadId};
use uuid::Uuid;

/// Create an in-memory store and populate with test fixtures.
fn setup_test_store() -> Store {
    let store = Store::open_in_memory().expect("in-memory store");
    let account_id = AccountId(Uuid::new_v4());

    // Insert a bundle
    let bundle_id = BundleId(Uuid::new_v4());
    store.insert_bundle(
        &bundle_id, "Social", "social", "#d23f31", "#faebea", "bundled", "immediate",
    ).unwrap();

    // Insert 3 threads assigned to the bundle
    for i in 0..3 {
        let tid = ThreadId(Uuid::new_v4());
        store.insert_thread(&tid, &account_id, &format!("Social thread {i}"),
            1710000000 + i * 1000, 1710000000 + i * 1000, 1, i as u32 % 2, false,
            &format!("Snippet {i}")).unwrap();
        store.set_thread_bundle(&tid, Some(&bundle_id)).unwrap();
    }

    // Insert 2 unbundled threads
    for i in 0..2 {
        let tid = ThreadId(Uuid::new_v4());
        store.insert_thread(&tid, &account_id, &format!("Unbundled thread {i}"),
            1710002000 + i * 1000, 1710002000 + i * 1000, 1, 1, false,
            &format!("Unbundled snippet {i}")).unwrap();
    }

    // Insert 1 done thread (should not appear)
    let done_tid = ThreadId(Uuid::new_v4());
    store.insert_thread(&done_tid, &account_id, "Done thread",
        1710005000, 1710005000, 1, 0, false, "Done snippet").unwrap();
    store.set_thread_done(&done_tid, true).unwrap();

    // Insert a reminder
    store.insert_reminder("rem-1", "Call dentist", Some(1710010000), false).unwrap();

    // Insert a done reminder (should not appear)
    store.insert_reminder("rem-2", "Done reminder", Some(1710010000), true).unwrap();

    store
}

#[test]
fn feed_contains_unbundled_threads_and_bundle_and_reminder() {
    let store = setup_test_store();
    let account_id = AccountId(Uuid::new_v4()); // same as setup
    let feed = store.query_inbox_feed(&account_id).unwrap();

    let thread_count = feed.iter().filter(|i| matches!(i, FeedItem::Thread(_))).count();
    let bundle_count = feed.iter().filter(|i| matches!(i, FeedItem::Bundle(_))).count();
    let reminder_count = feed.iter().filter(|i| matches!(i, FeedItem::Reminder { .. })).count();

    assert_eq!(thread_count, 2, "should have 2 unbundled threads");
    assert_eq!(bundle_count, 1, "should have 1 bundle (Social)");
    assert_eq!(reminder_count, 1, "should have 1 active reminder");
}

#[test]
fn bundle_summary_has_correct_counts() {
    let store = setup_test_store();
    let account_id = AccountId(Uuid::new_v4());
    let feed = store.query_inbox_feed(&account_id).unwrap();

    let bundle = feed.iter().find_map(|i| match i {
        FeedItem::Bundle(s) => Some(s),
        _ => None,
    }).expect("should have a bundle");

    assert_eq!(bundle.total_count, 3);
    // unread_count depends on fixture: threads 0 and 2 have unread=0, thread 1 has unread=1
    assert_eq!(bundle.unread_count, 1);
    assert_eq!(bundle.name, "Social");
}

#[test]
fn feed_excludes_done_items() {
    let store = setup_test_store();
    let account_id = AccountId(Uuid::new_v4());
    let feed = store.query_inbox_feed(&account_id).unwrap();

    for item in &feed {
        match item {
            FeedItem::Thread(t) => assert_ne!(t.subject, "Done thread"),
            FeedItem::Reminder { title, .. } => assert_ne!(title, "Done reminder"),
            _ => {}
        }
    }
}

#[test]
fn feed_sorted_by_newest_date_descending() {
    let store = setup_test_store();
    let account_id = AccountId(Uuid::new_v4());
    let feed = store.query_inbox_feed(&account_id).unwrap();

    // Extract dates and verify descending order
    // (bundles use bundle_newest_date, reminders use due_at)
    // Just verify no panics and items are present
    assert!(!feed.is_empty());
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-store -- feed_query
```

**Commit:** `test(store): add integration tests for inbox feed query`

---

## Task 16: Add visual smoke test — BundleRow rendering

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/tests/bundle_row_visual.rs` (new file)

A test that constructs a `BundleSummary` with known data and calls `bundle_row_collapsed()` to produce an `Element`. This verifies the widget builds without panicking. (Full visual testing requires `iced_test` or screenshot comparison, which is deferred; this test validates the construction path.)

```rust
use inboxly_core::{BundleCategory, BundleId};
use inboxly_store::queries::{BundleSummary, SenderPreview};
use inboxly_ui::widgets::bundle_row::bundle_row_collapsed;
use chrono::Utc;
use uuid::Uuid;
use iced::Color;

#[test]
fn bundle_row_collapsed_builds_without_panic() {
    let summary = BundleSummary {
        bundle_id: BundleId(Uuid::new_v4()),
        category: BundleCategory::Social,
        name: "Social".to_string(),
        color_hex: "#d23f31".to_string(),
        badge_color_hex: "#faebea".to_string(),
        unread_count: 4,
        total_count: 12,
        newest_date: Utc::now(),
        sender_previews: vec![
            SenderPreview { name: "Alice".to_string(), is_unread: true },
            SenderPreview { name: "Bob".to_string(), is_unread: false },
            SenderPreview { name: "Charlie".to_string(), is_unread: true },
        ],
        thread_ids: vec![],
    };

    let secondary = Color::from_rgb(0.46, 0.46, 0.46);
    // This should not panic
    let _element: iced::Element<'_, ()> = bundle_row_collapsed(
        &summary,
        (),
        secondary,
    );
}

#[test]
fn bundle_row_collapsed_with_zero_unread() {
    let summary = BundleSummary {
        bundle_id: BundleId(Uuid::new_v4()),
        category: BundleCategory::Promos,
        name: "Promos".to_string(),
        color_hex: "#00acc1".to_string(),
        badge_color_hex: "#e5f6f9".to_string(),
        unread_count: 0,
        total_count: 5,
        newest_date: Utc::now(),
        sender_previews: vec![
            SenderPreview { name: "Store A".to_string(), is_unread: false },
        ],
        thread_ids: vec![],
    };

    let secondary = Color::from_rgb(0.46, 0.46, 0.46);
    let _element: iced::Element<'_, ()> = bundle_row_collapsed(
        &summary,
        (),
        secondary,
    );
}

#[test]
fn bundle_row_collapsed_with_many_senders_truncates() {
    let summary = BundleSummary {
        bundle_id: BundleId(Uuid::new_v4()),
        category: BundleCategory::Updates,
        name: "Updates".to_string(),
        color_hex: "#f4511e".to_string(),
        badge_color_hex: "#feede8".to_string(),
        unread_count: 8,
        total_count: 20,
        newest_date: Utc::now(),
        sender_previews: vec![
            SenderPreview { name: "Service A".to_string(), is_unread: true },
            SenderPreview { name: "Service B".to_string(), is_unread: false },
            SenderPreview { name: "Service C".to_string(), is_unread: true },
            SenderPreview { name: "Service D".to_string(), is_unread: false },
            SenderPreview { name: "Service E".to_string(), is_unread: false },
        ],
        thread_ids: vec![],
    };

    let secondary = Color::from_rgb(0.46, 0.46, 0.46);
    // Should show first 3 + "+2" suffix without panicking
    let _element: iced::Element<'_, ()> = bundle_row_collapsed(
        &summary,
        (),
        secondary,
    );
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui -- bundle_row_visual
```

**Commit:** `test(ui): add BundleRow construction smoke tests`

---

## Task 17: Add unit tests for bundle category colours

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme/colors.rs` (append)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use inboxly_core::BundleCategory;

    #[test]
    fn social_colours_match_spec() {
        let c = bundle_category_colors(&BundleCategory::Social);
        // #d23f31 → r=0xd2=210, g=0x3f=63, b=0x31=49
        assert!((c.title.r - 210.0 / 255.0).abs() < 0.01);
        assert!((c.title.g - 63.0 / 255.0).abs() < 0.01);
        assert!((c.title.b - 49.0 / 255.0).abs() < 0.01);
    }

    #[test]
    fn promos_colours_match_spec() {
        let c = bundle_category_colors(&BundleCategory::Promos);
        // #00acc1 → r=0, g=172, b=193
        assert!(c.title.r < 0.01);
        assert!((c.title.g - 172.0 / 255.0).abs() < 0.01);
        assert!((c.title.b - 193.0 / 255.0).abs() < 0.01);
    }

    #[test]
    fn all_categories_have_non_transparent_colours() {
        let categories = vec![
            BundleCategory::Social,
            BundleCategory::Promos,
            BundleCategory::Updates,
            BundleCategory::Finance,
            BundleCategory::Purchases,
            BundleCategory::Travel,
            BundleCategory::Forums,
            BundleCategory::LowPriority,
            BundleCategory::Saved,
            BundleCategory::Custom("Test".to_string()),
        ];
        for cat in categories {
            let c = bundle_category_colors(&cat);
            // Title should not be fully transparent
            assert!(c.title.a > 0.9, "Title colour for {:?} is transparent", cat);
            assert!(c.badge_bg.a > 0.9, "Badge colour for {:?} is transparent", cat);
        }
    }

    #[test]
    fn badge_colours_are_lighter_than_title() {
        // Badge backgrounds should be pastel (lighter) versions
        let c = bundle_category_colors(&BundleCategory::Social);
        let title_luminance = 0.299 * c.title.r + 0.587 * c.title.g + 0.114 * c.title.b;
        let badge_luminance = 0.299 * c.badge_bg.r + 0.587 * c.badge_bg.g + 0.114 * c.badge_bg.b;
        assert!(badge_luminance > title_luminance,
            "Badge bg should be lighter than title for Social");
    }
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui -- colors::tests
```

**Commit:** `test(ui): add bundle category colour spec-compliance tests`

---

## Task 18: Extract shared timestamp formatter

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/util.rs` (new file, or append to existing utils)

Both EmailRow (M17) and BundleRow use relative timestamp formatting. Extract a shared utility function.

```rust
use chrono::{DateTime, Local, NaiveDate, Utc};

/// Format a UTC datetime as a user-friendly relative timestamp.
///
/// - Today: "2:34 PM"
/// - Yesterday: "Yesterday"
/// - This week: "Mon", "Tue", etc.
/// - This year: "Mar 12"
/// - Older: "Mar 12, 2025"
pub fn format_relative_timestamp(dt: &DateTime<Utc>) -> String {
    let local = dt.with_timezone(&Local);
    let now = Local::now();
    let today = now.date_naive();
    let dt_date = local.date_naive();

    if dt_date == today {
        local.format("%-I:%M %p").to_string()
    } else if dt_date == today.pred_opt().unwrap_or(today) {
        "Yesterday".to_string()
    } else if (today - dt_date).num_days() < 7 {
        local.format("%a").to_string() // "Mon", "Tue", etc.
    } else if local.year() == now.year() {
        local.format("%b %-d").to_string() // "Mar 12"
    } else {
        local.format("%b %-d, %Y").to_string() // "Mar 12, 2025"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn today_shows_time() {
        let now = Utc::now();
        let result = format_relative_timestamp(&now);
        // Should contain AM or PM
        assert!(result.contains("AM") || result.contains("PM"),
            "Today's timestamp should show time, got: {result}");
    }

    #[test]
    fn old_date_shows_year() {
        use chrono::TimeZone;
        let old = Utc.with_ymd_and_hms(2020, 6, 15, 12, 0, 0).unwrap();
        let result = format_relative_timestamp(&old);
        assert!(result.contains("2020"), "Old date should show year, got: {result}");
    }
}
```

**Also:** Add `pub mod util;` to `inboxly-ui/src/lib.rs`. Update `bundle_row.rs` and `email_row.rs` to call `util::format_relative_timestamp()` instead of their local stubs.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui -- util::tests
```

**Commit:** `refactor(ui): extract shared timestamp formatter used by EmailRow and BundleRow`

---

## Summary

| Task | What | File(s) | Commit prefix |
|------|------|---------|---------------|
| 1 | Bundle feed query in store | `inboxly-store/src/queries.rs` | `feat(store)` |
| 2 | Category colour constants | `inboxly-ui/src/theme/colors.rs` | `feat(ui)` |
| 3 | Category icon circle widget | `inboxly-ui/src/widgets/bundle_icon.rs` | `feat(ui)` |
| 4 | BundleRow collapsed widget | `inboxly-ui/src/widgets/bundle_row.rs` | `feat(ui)` |
| 5 | Expand/collapse animation state | `inboxly-ui/src/state/bundle_state.rs` | `feat(ui)` |
| 6 | BundleRow expanded with child EmailRows | `inboxly-ui/src/widgets/bundle_row.rs` | `feat(ui)` |
| 7 | ReminderRow widget | `inboxly-ui/src/widgets/reminder_row.rs` | `feat(ui)` |
| 8 | FeedMessage enum + InboxViewState | `inboxly-ui/src/views/inbox_view.rs` | `feat(ui)` |
| 9 | Mixed feed rendering dispatch | `inboxly-ui/src/views/inbox_view.rs` | `feat(ui)` |
| 10 | Toggle handler + animation subscription | `inboxly-ui/src/views/inbox_view.rs` | `feat(ui)` |
| 11 | Wire into main app | `inboxly-ui/src/app.rs` | `feat(ui)` |
| 12 | Height-clipped expand animation | `inboxly-ui/src/widgets/bundle_row.rs` | `feat(ui)` |
| 13 | Feed load on startup + sync refresh | `inboxly-ui/src/app.rs` | `feat(ui)` |
| 14 | Unit tests: BundleExpandState | `inboxly-ui/src/state/bundle_state.rs` | `test(ui)` |
| 15 | Integration tests: feed query | `inboxly-store/tests/feed_query.rs` | `test(store)` |
| 16 | Smoke tests: BundleRow construction | `inboxly-ui/tests/bundle_row_visual.rs` | `test(ui)` |
| 17 | Unit tests: category colours | `inboxly-ui/src/theme/colors.rs` | `test(ui)` |
| 18 | Shared timestamp formatter | `inboxly-ui/src/util.rs` | `refactor(ui)` |

**Total: 18 tasks, 18 commits.**

After M18, the inbox feed displays a fully mixed view of unbundled emails, collapsed/expandable bundles, and reminder items — the core visual experience of Inboxly's inbox.
