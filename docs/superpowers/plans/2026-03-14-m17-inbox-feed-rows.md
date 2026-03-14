# M17: Inbox Feed + Email Rows — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan.

**Goal:** Render a scrollable inbox feed with email rows and date-grouped section headers — the first time real data from the SQLite store appears on screen.

**Architecture:** All new code lives in `inboxly-ui`. The feed queries `inboxly-store` for threads, thread_state, and contacts (avatar data). The feed is a scrollable Iced `Column` inside the main content area (right of the 264dp nav drawer, below the 56dp toolbar). Each item is either an `EmailRow` widget or a `SectionHeader` widget. Date grouping logic categorises threads by temporal proximity (Pinned / Today / Yesterday / This Month / Earlier). Pinned threads are separated into their own section at the top.

**Tech Stack:** Rust, iced (0.13+), inboxly-store (rusqlite), inboxly-core types

**Prerequisites:**
- M15 complete — Iced shell running with toolbar (56dp) + nav drawer (264dp) + main content area placeholder
- M16 complete — `InboxlyTheme` with all colour tokens, light/dark, avatar letter colours
- M3 complete — SQLite schema + Store API (emails, threads, thread_state, contacts tables)
- M11 complete — Contacts + avatar system (avatar_letter, avatar_color_index in contacts table)

---

## Task 1: Add inboxly-store dependency to inboxly-ui

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/Cargo.toml`

Add `inboxly-store` and `chrono` to the `[dependencies]` section. The UI crate needs store access to query threads and contacts for the feed, and chrono for date grouping logic.

```toml
inboxly-store = { path = "../inboxly-store" }
chrono = { version = "0.4", features = ["serde"] }
```

`inboxly-core` should already be a dependency from M15. Verify it is present; if not, add it:

```toml
inboxly-core = { path = "../inboxly-core" }
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add inboxly-store and chrono dependencies for inbox feed`

---

## Task 2: Define DateGroup enum and date grouping logic

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/feed.rs` (new file)

Create the `feed` module with the `DateGroup` enum and a function that categorises a `DateTime<Utc>` into the appropriate group relative to "now". This drives the section headers in the inbox feed.

```rust
use chrono::{DateTime, Datelike, Local, NaiveDate, Utc};

/// Date-based grouping for inbox feed sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DateGroup {
    /// Pinned items — always at the top, regardless of date.
    Pinned,
    /// Received today (local time).
    Today,
    /// Received yesterday (local time).
    Yesterday,
    /// Received earlier this week (within 7 days, but not today/yesterday).
    ThisWeek,
    /// Received this month (same calendar month, but not this week).
    ThisMonth,
    /// Everything older than this month.
    Earlier,
}

impl DateGroup {
    /// Classify a thread date into a date group, relative to the current local time.
    pub fn from_date(date: DateTime<Utc>) -> Self {
        let now_local = Local::now();
        let date_local = date.with_timezone(&Local);

        let today = now_local.date_naive();
        let thread_date = date_local.date_naive();

        if thread_date == today {
            return Self::Today;
        }

        if thread_date == today.pred_opt().unwrap_or(today) {
            return Self::Yesterday;
        }

        let days_ago = (today - thread_date).num_days();
        if days_ago > 0 && days_ago <= 7 {
            return Self::ThisWeek;
        }

        if thread_date.year() == today.year() && thread_date.month() == today.month() {
            return Self::ThisMonth;
        }

        Self::Earlier
    }

    /// Display label for the section header.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pinned => "Pinned",
            Self::Today => "Today",
            Self::Yesterday => "Yesterday",
            Self::ThisWeek => "This Week",
            Self::ThisMonth => "This Month",
            Self::Earlier => "Earlier",
        }
    }
}
```

**Also:** Add `pub mod feed;` to `inboxly-ui/src/lib.rs`.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add DateGroup enum and date classification logic`

---

## Task 3: Define FeedItem and FeedSection types

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/feed.rs` (append)

Add the view-model types that the feed widget will render. `FeedItem` is the display representation of one row. `FeedSection` groups items under a `DateGroup` header.

```rust
use inboxly_core::types::{ThreadId, Contact};

/// A single displayable row in the inbox feed.
/// This is a view-model — lightweight data extracted from store queries,
/// not the full Thread/EmailMeta types.
#[derive(Debug, Clone)]
pub struct FeedItem {
    /// Thread ID for navigation and actions.
    pub thread_id: ThreadId,
    /// Primary sender's display name (or email if no name).
    pub sender_name: String,
    /// Primary sender's email address (for avatar lookup).
    pub sender_address: String,
    /// Avatar letter (first letter of name, uppercased).
    pub avatar_letter: char,
    /// Avatar colour index (0-25, maps to A-Z palette).
    pub avatar_color_index: u8,
    /// Thread subject line.
    pub subject: String,
    /// Snippet — first ~200 chars of newest email body, plaintext.
    pub snippet: String,
    /// Timestamp of newest email in thread.
    pub timestamp: DateTime<Utc>,
    /// Formatted timestamp for display (e.g., "2:30 PM", "Mar 12", "Dec 2025").
    pub timestamp_display: String,
    /// Whether the thread has unread emails.
    pub is_unread: bool,
    /// Whether the thread has attachments.
    pub has_attachments: bool,
    /// Whether the thread is pinned.
    pub is_pinned: bool,
    /// Number of emails in the thread (shown as count badge if > 1).
    pub email_count: u32,
}

/// A section of the feed — a date group header + its items.
#[derive(Debug, Clone)]
pub struct FeedSection {
    /// The date group this section represents.
    pub group: DateGroup,
    /// Items in this section, ordered newest-first.
    pub items: Vec<FeedItem>,
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add FeedItem and FeedSection view-model types`

---

## Task 4: Implement timestamp formatting

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/feed.rs` (append)

Add a function that formats a `DateTime<Utc>` into the appropriate display string based on how recent it is. Google Inbox uses relative formatting: time-only for today, date for this year, full date for older.

```rust
/// Format a timestamp for display in an email row.
///
/// Rules:
/// - Today: "2:30 PM" (12-hour format)
/// - Yesterday: "Yesterday"
/// - This week: weekday name ("Tuesday")
/// - This year: "Mar 12"
/// - Older: "Mar 12, 2025"
pub fn format_timestamp(date: DateTime<Utc>) -> String {
    let now_local = Local::now();
    let date_local = date.with_timezone(&Local);

    let today = now_local.date_naive();
    let thread_date = date_local.date_naive();

    if thread_date == today {
        return date_local.format("%-I:%M %p").to_string();
    }

    if thread_date == today.pred_opt().unwrap_or(today) {
        return "Yesterday".to_string();
    }

    let days_ago = (today - thread_date).num_days();
    if days_ago > 0 && days_ago <= 7 {
        return date_local.format("%A").to_string(); // "Tuesday"
    }

    if thread_date.year() == today.year() {
        return date_local.format("%b %-d").to_string(); // "Mar 12"
    }

    date_local.format("%b %-d, %Y").to_string() // "Mar 12, 2025"
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add relative timestamp formatting for email rows`

---

## Task 5: Implement feed data query from store

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/feed.rs` (append)

Add the function that queries the store to build the feed. This joins `threads`, `thread_state`, and `contacts` tables to produce a `Vec<FeedSection>`. This is the critical data bridge — the first time the UI reads real data from SQLite.

The query logic:
1. Query all non-done threads (thread_state.done = false OR no thread_state row) ordered by newest_date DESC.
2. For each thread, look up the sender's contact record (avatar_letter, avatar_color_index).
3. Separate pinned threads into the Pinned section.
4. Group remaining threads by DateGroup based on newest_date.
5. Return sections in display order: Pinned, Today, Yesterday, ThisWeek, ThisMonth, Earlier. Omit empty sections.

```rust
use inboxly_store::Store;
use std::collections::BTreeMap;

/// Query the store and build the feed sections for the inbox view.
///
/// Returns sections in display order (Pinned first, then chronological).
/// Empty sections are omitted.
pub fn build_feed(store: &Store) -> Result<Vec<FeedSection>, inboxly_store::Error> {
    // Query active (non-done) threads with their state and sender contact info.
    // This is a single query joining threads, thread_state, emails, and contacts.
    let threads = store.query_inbox_threads()?;

    let mut pinned_items = Vec::new();
    let mut grouped: BTreeMap<DateGroup, Vec<FeedItem>> = BTreeMap::new();

    for thread in threads {
        let item = FeedItem {
            thread_id: thread.id.clone(),
            sender_name: thread.sender_name.clone(),
            sender_address: thread.sender_address.clone(),
            avatar_letter: thread.avatar_letter,
            avatar_color_index: thread.avatar_color_index,
            subject: thread.subject.clone(),
            snippet: thread.snippet.clone(),
            timestamp: thread.newest_date,
            timestamp_display: format_timestamp(thread.newest_date),
            is_unread: thread.unread_count > 0,
            has_attachments: thread.has_attachments,
            is_pinned: thread.pinned,
            email_count: thread.email_count,
        };

        if thread.pinned {
            pinned_items.push(item);
        } else {
            let group = DateGroup::from_date(thread.newest_date);
            grouped.entry(group).or_default().push(item);
        }
    }

    let mut sections = Vec::new();

    // Pinned section first (if any pinned items exist).
    if !pinned_items.is_empty() {
        sections.push(FeedSection {
            group: DateGroup::Pinned,
            items: pinned_items,
        });
    }

    // Chronological sections in order.
    let group_order = [
        DateGroup::Today,
        DateGroup::Yesterday,
        DateGroup::ThisWeek,
        DateGroup::ThisMonth,
        DateGroup::Earlier,
    ];

    for group in group_order {
        if let Some(items) = grouped.remove(&group) {
            if !items.is_empty() {
                sections.push(FeedSection { group, items });
            }
        }
    }

    Ok(sections)
}
```

**Store API requirement:** This task assumes `inboxly-store` exposes a `query_inbox_threads()` method that returns a flat list of thread summaries with joined contact data. If this method does not yet exist, it must be added to `inboxly-store` (see Task 6).

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement feed data query and section grouping`

---

## Task 6: Add query_inbox_threads to inboxly-store

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/lib.rs` (or appropriate query module)

Add the store method that the feed builder depends on. This executes a single SQL query joining `threads`, `thread_state`, `emails` (for sender), and `contacts` (for avatar).

```rust
/// Summary of a thread for inbox feed display.
/// Joins thread metadata with state and sender contact info.
#[derive(Debug, Clone)]
pub struct InboxThreadSummary {
    pub id: ThreadId,
    pub subject: String,
    pub snippet: String,
    pub newest_date: DateTime<Utc>,
    pub email_count: u32,
    pub unread_count: u32,
    pub has_attachments: bool,
    pub pinned: bool,
    pub sender_name: String,
    pub sender_address: String,
    pub avatar_letter: char,
    pub avatar_color_index: u8,
}
```

The SQL query:

```sql
SELECT
    t.id,
    t.subject,
    t.snippet,
    t.newest_date,
    t.email_count,
    t.unread_count,
    t.has_attachments,
    COALESCE(ts.pinned, 0) AS pinned,
    COALESCE(ts.done, 0) AS done,
    -- Subquery: get the from_address of the newest email in this thread
    (SELECT e.from_address FROM emails e
     WHERE e.thread_id = t.id
     ORDER BY e.date DESC LIMIT 1) AS sender_address,
    (SELECT e.from_name FROM emails e
     WHERE e.thread_id = t.id
     ORDER BY e.date DESC LIMIT 1) AS sender_name,
    -- Join contact for avatar
    COALESCE(c.avatar_letter, UPPER(SUBSTR(
        COALESCE(
            (SELECT e.from_name FROM emails e WHERE e.thread_id = t.id ORDER BY e.date DESC LIMIT 1),
            (SELECT e.from_address FROM emails e WHERE e.thread_id = t.id ORDER BY e.date DESC LIMIT 1)
        ), 1, 1
    ))) AS avatar_letter,
    COALESCE(c.avatar_color_index, 0) AS avatar_color_index
FROM threads t
LEFT JOIN thread_state ts ON t.id = ts.thread_id
LEFT JOIN contacts c ON c.address = (
    SELECT e.from_address FROM emails e
    WHERE e.thread_id = t.id
    ORDER BY e.date DESC LIMIT 1
)
WHERE COALESCE(ts.done, 0) = 0
ORDER BY t.newest_date DESC
```

Implement `Store::query_inbox_threads(&self) -> Result<Vec<InboxThreadSummary>, Error>` by preparing this statement and mapping rows to `InboxThreadSummary`.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-store
```

**Commit:** `feat(store): add query_inbox_threads for inbox feed display`

---

## Task 7: Implement avatar circle rendering

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/avatar.rs` (new file)

Create a reusable avatar widget. The avatar is a 40dp diameter circle filled with the sender's colour (from the A-Z palette), with a single uppercase letter centered inside in white.

```rust
use iced::widget::canvas::{self, Canvas, Frame, Path};
use iced::{Color, Element, Length, Point, Size, Theme};

/// Avatar circle dimensions from the spec.
pub const AVATAR_DIAMETER: f32 = 40.0;

/// A-Z avatar colour palette from the BigTop APK.
const AVATAR_COLORS: [Color; 27] = [
    Color::from_rgb(0.878, 0.376, 0.333),  // A=#e06055
    Color::from_rgb(0.929, 0.380, 0.573),  // B=#ed6192
    Color::from_rgb(0.729, 0.408, 0.784),  // C=#ba68c8
    Color::from_rgb(0.584, 0.459, 0.804),  // D=#9575cd
    Color::from_rgb(0.475, 0.525, 0.796),  // E=#7986cb
    Color::from_rgb(0.369, 0.592, 0.965),  // F=#5e97f6
    Color::from_rgb(0.310, 0.765, 0.969),  // G=#4fc3f7
    Color::from_rgb(0.345, 0.816, 0.882),  // H=#58d0e1
    Color::from_rgb(0.310, 0.714, 0.675),  // I=#4fb6ac
    Color::from_rgb(0.341, 0.733, 0.541),  // J=#57bb8a
    Color::from_rgb(0.612, 0.800, 0.396),  // K=#9ccc65
    Color::from_rgb(0.831, 0.882, 0.341),  // L=#d4e157
    Color::from_rgb(0.992, 0.847, 0.208),  // M=#fdd835
    Color::from_rgb(0.965, 0.749, 0.196),  // N=#f6bf32
    Color::from_rgb(0.961, 0.651, 0.192),  // O=#f5a631
    Color::from_rgb(0.945, 0.533, 0.392),  // P=#f18864
    Color::from_rgb(0.761, 0.761, 0.761),  // Q=#c2c2c2
    Color::from_rgb(0.565, 0.643, 0.682),  // R=#90a4ae
    Color::from_rgb(0.631, 0.533, 0.498),  // S=#a1887f
    Color::from_rgb(0.639, 0.639, 0.639),  // T=#a3a3a3
    Color::from_rgb(0.686, 0.714, 0.878),  // U=#afb6e0
    Color::from_rgb(0.702, 0.616, 0.859),  // V=#b39ddb
    Color::from_rgb(0.761, 0.761, 0.761),  // W=#c2c2c2
    Color::from_rgb(0.502, 0.871, 0.918),  // X=#80deea
    Color::from_rgb(0.737, 0.667, 0.643),  // Y=#bcaaa4
    Color::from_rgb(0.682, 0.835, 0.506),  // Z=#aed581
    Color::from_rgb(0.937, 0.937, 0.937),  // default=#efefef
];

/// Get the avatar background colour for a given colour index (0-25 for A-Z, 26 for default).
pub fn avatar_color(index: u8) -> Color {
    AVATAR_COLORS[index.min(26) as usize]
}

/// Avatar widget state for canvas rendering.
pub struct Avatar {
    letter: char,
    color_index: u8,
}

impl Avatar {
    pub fn new(letter: char, color_index: u8) -> Self {
        Self { letter, color_index }
    }
}
```

The actual rendering uses Iced's `Canvas` widget to draw:
1. A filled circle with the avatar colour.
2. A white letter centered in the circle.

The canvas `draw` implementation:

```rust
impl<Message> canvas::Program<Message> for Avatar {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: iced::Rectangle,
        _cursor: canvas::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);
        let radius = bounds.width.min(bounds.height) / 2.0;

        // Draw filled circle.
        let circle = Path::circle(center, radius);
        frame.fill(&circle, avatar_color(self.color_index));

        // Draw letter — use iced's text rendering.
        frame.fill_text(canvas::Text {
            content: self.letter.to_string(),
            position: center,
            color: Color::WHITE,
            size: iced::Pixels(20.0), // ~20sp inside 40dp circle
            horizontal_alignment: iced::alignment::Horizontal::Center,
            vertical_alignment: iced::alignment::Vertical::Center,
            ..Default::default()
        });

        vec![frame.into_geometry()]
    }
}
```

Provide a convenience function to create the avatar element:

```rust
/// Create an avatar circle element with the given letter and colour index.
pub fn avatar_circle<'a, Message: 'a>(
    letter: char,
    color_index: u8,
) -> Element<'a, Message> {
    Canvas::new(Avatar::new(letter, color_index))
        .width(Length::Fixed(AVATAR_DIAMETER))
        .height(Length::Fixed(AVATAR_DIAMETER))
        .into()
}
```

**Also:** Create the `widgets` module directory if it does not exist. Add `pub mod avatar;` to `inboxly-ui/src/widgets/mod.rs` and `pub mod widgets;` to `inboxly-ui/src/lib.rs`.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement avatar circle widget with A-Z colour palette`

---

## Task 8: Implement SectionHeader widget

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/section_header.rs` (new file)

Create the `SectionHeader` widget. Per the spec: 48dp height, 14sp bold grey text, left-aligned with 16dp padding. The header displays the date group label ("Pinned", "Today", "Yesterday", etc.).

```rust
use iced::widget::{container, row, text};
use iced::{Alignment, Element, Length, Padding};

use crate::feed::DateGroup;

/// Height of a section header row (from spec dimensions).
const SECTION_HEADER_HEIGHT: f32 = 48.0;

/// Font size for section header text (from spec typography).
const SECTION_HEADER_FONT_SIZE: f32 = 14.0;

/// Default horizontal padding (from spec dimensions).
const DEFAULT_PADDING: f32 = 16.0;

/// Build a section header element for the given date group.
///
/// Renders as a 48dp tall row with bold grey text (14sp) left-aligned.
/// The label comes from DateGroup::label().
pub fn section_header<'a, Message: 'a>(
    group: DateGroup,
    secondary_text_color: iced::Color,
) -> Element<'a, Message> {
    let label = text(group.label())
        .size(SECTION_HEADER_FONT_SIZE)
        .style(move |_theme| text::Style {
            color: Some(secondary_text_color),
        })
        .font(iced::Font {
            weight: iced::font::Weight::Bold,
            ..Default::default()
        });

    container(
        row![label]
            .align_y(Alignment::Center)
    )
    .height(SECTION_HEADER_HEIGHT)
    .width(Length::Fill)
    .padding(Padding {
        top: 0.0,
        right: DEFAULT_PADDING,
        bottom: 0.0,
        left: DEFAULT_PADDING,
    })
    .align_y(iced::alignment::Vertical::Center)
    .into()
}
```

**Also:** Add `pub mod section_header;` to `inboxly-ui/src/widgets/mod.rs`.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement SectionHeader widget (48dp, 14sp bold grey)`

---

## Task 9: Implement EmailRow widget

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/email_row.rs` (new file)

Create the `EmailRow` widget — the core visual element of the inbox feed. Layout from the spec:

```
┌─────────────────────────────────────────────────────────────┐
│ [  Avatar  ] Sender Name          Timestamp   📎           │
│ [ 40dp ⌀  ] Subject — Snippet preview text...              │
│ [72dp col ]                                                 │
└─────────────────────────────────────────────────────────────┘
```

Dimensions and typography (from spec):
- Avatar column width: 72dp (40dp avatar + 16dp left padding + 16dp right gap)
- Sender name: 16sp, bold if unread, primary text colour
- Subject: 14sp, primary text colour
- Snippet: 14sp, secondary text colour (grey), follows subject with " — " separator
- Timestamp: 12sp, secondary text colour, right-aligned
- Attachment indicator: paperclip icon or "📎" text, 12sp, shown if has_attachments
- Card elevation: 2dp (surface colour background with divider below)
- Unread: sender name uses bold weight

```rust
use iced::widget::{column, container, row, text, Space};
use iced::{Alignment, Color, Element, Length, Padding};

use crate::feed::FeedItem;
use crate::widgets::avatar::avatar_circle;

/// Avatar column width (40dp avatar + 16dp padding each side).
const AVATAR_COLUMN_WIDTH: f32 = 72.0;
/// Default padding.
const DEFAULT_PADDING: f32 = 16.0;

/// Message type for email row interactions.
#[derive(Debug, Clone)]
pub enum EmailRowMessage {
    /// User clicked on this email row to open the thread.
    Clicked(inboxly_core::types::ThreadId),
}

/// Build an email row element from a FeedItem.
///
/// Layout: avatar (72dp column) | content (sender, subject+snippet, flexible) | timestamp + attachment (right-aligned)
///
/// Colours are passed in from the theme to keep this widget theme-agnostic.
pub fn email_row<'a>(
    item: &FeedItem,
    primary_text_color: Color,
    secondary_text_color: Color,
    surface_color: Color,
    divider_color: Color,
) -> Element<'a, EmailRowMessage> {
    // --- Avatar column (72dp wide) ---
    let avatar = container(
        avatar_circle(item.avatar_letter, item.avatar_color_index)
    )
    .width(AVATAR_COLUMN_WIDTH)
    .padding(Padding {
        top: 0.0,
        right: 16.0, // gap between avatar and content
        bottom: 0.0,
        left: DEFAULT_PADDING,
    })
    .align_y(iced::alignment::Vertical::Center);

    // --- Sender name (16sp, bold if unread) ---
    let sender_weight = if item.is_unread {
        iced::font::Weight::Bold
    } else {
        iced::font::Weight::Normal
    };

    let sender = text(&item.sender_name)
        .size(16.0)
        .style(move |_theme| text::Style {
            color: Some(primary_text_color),
        })
        .font(iced::Font {
            weight: sender_weight,
            ..Default::default()
        });

    // --- Subject + Snippet (14sp) ---
    // Subject in primary colour, snippet in secondary (grey).
    // Combined as "Subject — Snippet..." on one line.
    let subject_snippet = {
        let combined = if item.snippet.is_empty() {
            item.subject.clone()
        } else {
            format!("{} — {}", item.subject, item.snippet)
        };

        // Truncate to a reasonable display length.
        let display = if combined.len() > 120 {
            format!("{}…", &combined[..120])
        } else {
            combined
        };

        text(display)
            .size(14.0)
            .style(move |_theme| text::Style {
                color: Some(secondary_text_color),
            })
    };

    // --- Content column (sender on top, subject+snippet below) ---
    let content = column![sender, subject_snippet]
        .spacing(2.0)
        .width(Length::Fill);

    // --- Right column: timestamp + attachment indicator ---
    let timestamp = text(&item.timestamp_display)
        .size(12.0)
        .style(move |_theme| text::Style {
            color: Some(secondary_text_color),
        });

    let mut right_col = column![timestamp]
        .align_x(iced::Alignment::End)
        .spacing(4.0);

    if item.has_attachments {
        let attachment_icon = text("📎")
            .size(12.0)
            .style(move |_theme| text::Style {
                color: Some(secondary_text_color),
            });
        right_col = right_col.push(attachment_icon);
    }

    // --- Assemble full row ---
    let row_content = row![avatar, content, right_col]
        .align_y(Alignment::Center)
        .spacing(0.0)
        .padding(Padding {
            top: 12.0,
            right: DEFAULT_PADDING,
            bottom: 12.0,
            left: 0.0, // avatar container handles left padding
        });

    // Wrap in a container with surface background and bottom divider.
    // The divider is a 1px container below the row.
    let row_with_bg = container(row_content)
        .width(Length::Fill)
        .style(move |_theme| container::Style {
            background: Some(iced::Background::Color(surface_color)),
            ..Default::default()
        });

    let divider = container(Space::new(Length::Fill, 1.0))
        .style(move |_theme| container::Style {
            background: Some(iced::Background::Color(divider_color)),
            ..Default::default()
        });

    // Stack row + divider vertically.
    column![row_with_bg, divider]
        .width(Length::Fill)
        .into()
}
```

**Also:** Add `pub mod email_row;` to `inboxly-ui/src/widgets/mod.rs`.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement EmailRow widget with avatar, sender, subject, snippet, timestamp`

---

## Task 10: Implement empty state ("Inbox Zero" placeholder)

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/empty_state.rs` (new file)

Create the empty state view shown when the inbox has zero items. This is a centered placeholder — the full Inbox Zero sun illustration is a later milestone (M25). For now, display a simple centered text message.

```rust
use iced::widget::{center, column, text, Space};
use iced::{Color, Element, Length};

/// Build an empty state element for when the inbox has no items.
///
/// Displays a centered message. The full "Inbox Zero Sun" illustration
/// is deferred to M25 — this is a text placeholder.
pub fn empty_inbox<'a, Message: 'a>(
    secondary_text_color: Color,
) -> Element<'a, Message> {
    let heading = text("You're all done!")
        .size(24.0)
        .style(move |_theme| text::Style {
            color: Some(secondary_text_color),
        })
        .font(iced::Font {
            weight: iced::font::Weight::Bold,
            ..Default::default()
        });

    let subtext = text("Nothing in your inbox. Enjoy your day.")
        .size(16.0)
        .style(move |_theme| text::Style {
            color: Some(secondary_text_color),
        });

    center(
        column![heading, Space::with_height(8.0), subtext]
            .align_x(iced::Alignment::Center)
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
```

**Also:** Add `pub mod empty_state;` to `inboxly-ui/src/widgets/mod.rs`.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add empty inbox placeholder for zero-item state`

---

## Task 11: Implement the scrollable inbox feed view

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/inbox_view.rs` (new file)

Create the inbox view — the main content area that combines section headers and email rows into a scrollable feed. This replaces the placeholder content area from M15.

```rust
use iced::widget::{column, scrollable, Column};
use iced::{Color, Element, Length};

use crate::feed::{build_feed, FeedSection};
use crate::widgets::email_row::{email_row, EmailRowMessage};
use crate::widgets::empty_state::empty_inbox;
use crate::widgets::section_header::section_header;

use inboxly_store::Store;

/// Message type for the inbox view.
#[derive(Debug, Clone)]
pub enum InboxViewMessage {
    /// An email row was interacted with.
    EmailRow(EmailRowMessage),
}

/// Build the scrollable inbox feed view.
///
/// Queries the store for active threads, groups them by date,
/// and renders section headers + email rows.
///
/// Theme colours are passed in from the app-level theme.
pub fn inbox_view<'a>(
    sections: &[FeedSection],
    primary_text_color: Color,
    secondary_text_color: Color,
    surface_color: Color,
    divider_color: Color,
    background_color: Color,
) -> Element<'a, InboxViewMessage> {
    // Empty state.
    if sections.is_empty() {
        return empty_inbox(secondary_text_color)
            .map(|_: std::convert::Infallible| unreachable!());
    }

    // Build the feed column: alternating section headers and email rows.
    let mut feed_column = Column::new()
        .width(Length::Fill)
        .spacing(0.0);

    for section in sections {
        // Section header.
        feed_column = feed_column.push(
            section_header(section.group, secondary_text_color)
        );

        // Email rows within this section.
        for item in &section.items {
            feed_column = feed_column.push(
                email_row(
                    item,
                    primary_text_color,
                    secondary_text_color,
                    surface_color,
                    divider_color,
                ).map(InboxViewMessage::EmailRow)
            );
        }
    }

    // Wrap in a scrollable container.
    scrollable(feed_column)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
```

**Also:** Create the `views` module directory if it does not exist. Add `pub mod inbox_view;` to `inboxly-ui/src/views/mod.rs` and `pub mod views;` to `inboxly-ui/src/lib.rs`.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement scrollable inbox feed view with section headers and email rows`

---

## Task 12: Wire the inbox feed into the main app

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs` (modify — this is the main Iced Application from M15)

Integrate the inbox feed into the existing Iced application shell. This connects the store, loads feed data on startup, and renders the feed in the main content area (right of nav drawer, below toolbar).

Changes to the app state struct:

```rust
// Add to the Inboxly app state:
pub struct Inboxly {
    // ... existing fields from M15 (nav_selection, etc.)
    store: Arc<Store>,
    feed_sections: Vec<FeedSection>,
}
```

Changes to the `update` function — add a message variant to reload the feed:

```rust
#[derive(Debug, Clone)]
pub enum Message {
    // ... existing variants from M15
    LoadFeed,
    FeedLoaded(Vec<FeedSection>),
    InboxView(InboxViewMessage),
}
```

In `update`, handle `LoadFeed` by spawning a task that calls `build_feed(store)` and sends `FeedLoaded` back. Handle `FeedLoaded` by storing the sections.

In `view`, replace the main content area placeholder with the `inbox_view()` call, passing feed_sections and theme colours from the `InboxlyTheme`.

On app startup (`new` or `subscription`), dispatch `Message::LoadFeed` to trigger initial feed load.

**Key integration points:**
- The `Store` is created in the binary crate (`inboxly/src/main.rs`) and passed to the UI via `Inboxly::new()` or `Inboxly::run()`.
- Theme colours are read from `self.theme` (the `InboxlyTheme` from M16).
- `InboxViewMessage::EmailRow(EmailRowMessage::Clicked(thread_id))` is received but not yet handled (conversation view is a later milestone) — log it or no-op.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): wire inbox feed into main app shell, replace content placeholder`

---

## Task 13: Wire Store into the binary crate

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly/src/main.rs` (modify)

Update the binary crate to create the `Store` instance and pass it to the UI application. The store opens (or creates) the SQLite database at the XDG data directory path from the config system (M2).

```rust
use inboxly_store::Store;
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ... existing CLI arg parsing from M15

    // Open store at XDG data path.
    let data_dir = inboxly_core::config::data_dir();
    let store = Arc::new(Store::open(&data_dir)?);

    // Launch UI with store.
    inboxly_ui::Inboxly::run(store)?;

    Ok(())
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check
```

**Commit:** `feat(bin): wire Store creation into binary and pass to UI`

---

## Task 14: Add unit tests for DateGroup classification

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/feed.rs` (append tests module)

Add tests for the date grouping logic. Use fixed timestamps relative to a known "now" to verify each group boundary. These tests exercise the core logic without needing a database or UI.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone, Utc};

    #[test]
    fn test_today_classification() {
        let now = Utc::now();
        let group = DateGroup::from_date(now);
        assert_eq!(group, DateGroup::Today);
    }

    #[test]
    fn test_yesterday_classification() {
        let yesterday = Utc::now() - Duration::days(1);
        let group = DateGroup::from_date(yesterday);
        assert_eq!(group, DateGroup::Yesterday);
    }

    #[test]
    fn test_this_week_classification() {
        let three_days_ago = Utc::now() - Duration::days(3);
        let group = DateGroup::from_date(three_days_ago);
        // Should be either ThisWeek or Yesterday depending on exact day.
        assert!(
            group == DateGroup::ThisWeek || group == DateGroup::Yesterday,
            "3 days ago should be ThisWeek or Yesterday, got {:?}",
            group
        );

        let five_days_ago = Utc::now() - Duration::days(5);
        let group = DateGroup::from_date(five_days_ago);
        assert_eq!(group, DateGroup::ThisWeek);
    }

    #[test]
    fn test_earlier_classification() {
        let old = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();
        let group = DateGroup::from_date(old);
        assert_eq!(group, DateGroup::Earlier);
    }

    #[test]
    fn test_date_group_label() {
        assert_eq!(DateGroup::Pinned.label(), "Pinned");
        assert_eq!(DateGroup::Today.label(), "Today");
        assert_eq!(DateGroup::Yesterday.label(), "Yesterday");
        assert_eq!(DateGroup::ThisWeek.label(), "This Week");
        assert_eq!(DateGroup::ThisMonth.label(), "This Month");
        assert_eq!(DateGroup::Earlier.label(), "Earlier");
    }

    #[test]
    fn test_date_group_ordering() {
        // Pinned < Today < Yesterday < ThisWeek < ThisMonth < Earlier
        assert!(DateGroup::Pinned < DateGroup::Today);
        assert!(DateGroup::Today < DateGroup::Yesterday);
        assert!(DateGroup::Yesterday < DateGroup::ThisWeek);
        assert!(DateGroup::ThisWeek < DateGroup::ThisMonth);
        assert!(DateGroup::ThisMonth < DateGroup::Earlier);
    }
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui
```

**Commit:** `test(ui): add unit tests for DateGroup classification and ordering`

---

## Task 15: Add unit tests for timestamp formatting

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/feed.rs` (append to tests module)

Add tests for the `format_timestamp` function. These verify the relative formatting rules.

```rust
    #[test]
    fn test_format_timestamp_today() {
        let now = Utc::now();
        let formatted = format_timestamp(now);
        // Should contain AM or PM (12-hour format).
        assert!(
            formatted.contains("AM") || formatted.contains("PM"),
            "Today's timestamp should be time-only, got: {}",
            formatted
        );
    }

    #[test]
    fn test_format_timestamp_yesterday() {
        let yesterday = Utc::now() - Duration::days(1);
        let formatted = format_timestamp(yesterday);
        assert_eq!(formatted, "Yesterday");
    }

    #[test]
    fn test_format_timestamp_this_week() {
        let five_days_ago = Utc::now() - Duration::days(5);
        let formatted = format_timestamp(five_days_ago);
        // Should be a weekday name.
        let weekdays = [
            "Monday", "Tuesday", "Wednesday", "Thursday",
            "Friday", "Saturday", "Sunday",
        ];
        assert!(
            weekdays.iter().any(|d| formatted == *d),
            "5 days ago should be a weekday name, got: {}",
            formatted
        );
    }

    #[test]
    fn test_format_timestamp_old() {
        let old = Utc.with_ymd_and_hms(2020, 6, 15, 12, 0, 0).unwrap();
        let formatted = format_timestamp(old);
        assert_eq!(formatted, "Jun 15, 2020");
    }
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui
```

**Commit:** `test(ui): add unit tests for relative timestamp formatting`

---

## Task 16: Add integration test for feed building with in-memory store

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/tests/feed_integration.rs` (new file)

Create an integration test that:
1. Opens an in-memory SQLite store.
2. Inserts fixture threads, emails, contacts, and thread_state records.
3. Calls `build_feed()` and verifies the sections are correct.

This validates the full data path: SQLite → query_inbox_threads → build_feed → FeedSections.

```rust
use chrono::{Duration, Utc};
use inboxly_store::Store;
use inboxly_ui::feed::{build_feed, DateGroup};

#[test]
fn test_build_feed_groups_by_date() {
    let store = Store::open_in_memory().expect("in-memory store");

    // Insert a thread from today.
    let today_thread = store
        .insert_test_thread("Today thread", Utc::now(), false, false)
        .expect("insert today thread");

    // Insert a thread from yesterday.
    let yesterday_thread = store
        .insert_test_thread("Yesterday thread", Utc::now() - Duration::days(1), false, false)
        .expect("insert yesterday thread");

    // Insert a pinned thread.
    let pinned_thread = store
        .insert_test_thread("Pinned thread", Utc::now() - Duration::days(5), true, false)
        .expect("insert pinned thread");

    // Insert a done thread (should NOT appear in feed).
    let done_thread = store
        .insert_test_thread("Done thread", Utc::now(), false, true)
        .expect("insert done thread");

    let sections = build_feed(&store).expect("build_feed");

    // Should have Pinned, Today, Yesterday sections (3 sections).
    // Done thread should be excluded.
    assert_eq!(sections.len(), 3);
    assert_eq!(sections[0].group, DateGroup::Pinned);
    assert_eq!(sections[0].items.len(), 1);
    assert_eq!(sections[1].group, DateGroup::Today);
    assert_eq!(sections[1].items.len(), 1);
    assert_eq!(sections[2].group, DateGroup::Yesterday);
    assert_eq!(sections[2].items.len(), 1);
}

#[test]
fn test_build_feed_empty_inbox() {
    let store = Store::open_in_memory().expect("in-memory store");
    let sections = build_feed(&store).expect("build_feed");
    assert!(sections.is_empty());
}

#[test]
fn test_build_feed_unread_flag() {
    let store = Store::open_in_memory().expect("in-memory store");

    // Insert a thread with unread emails.
    store
        .insert_test_thread_with_unread("Unread thread", Utc::now(), 3)
        .expect("insert unread thread");

    let sections = build_feed(&store).expect("build_feed");
    assert_eq!(sections.len(), 1);
    assert!(sections[0].items[0].is_unread);
}
```

**Store test helpers:** This test assumes `Store` has `open_in_memory()` and `insert_test_thread()` / `insert_test_thread_with_unread()` helper methods. If these do not exist in inboxly-store from M3, add them behind `#[cfg(test)]` or a `test-helpers` feature flag.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui
```

**Commit:** `test(ui): add integration tests for feed building with fixture data`

---

## Task 17: Final verification and cleanup

No new code. Run full workspace checks to ensure everything compiles cleanly and all tests pass.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check --workspace
cd /mnt/TempNVME/projects/inbox-rust && cargo test --workspace
cd /mnt/TempNVME/projects/inbox-rust && cargo clippy --workspace -- -D warnings
```

Fix any warnings or errors from clippy. Common items to watch for:
- Unused imports (remove any `use` statements that were speculative).
- Missing `#[must_use]` annotations on public functions.
- Redundant clones.

**Commit (if fixes needed):** `fix(ui): address clippy warnings in inbox feed implementation`

---

## Summary

| Task | File(s) | What |
|------|---------|------|
| 1 | `inboxly-ui/Cargo.toml` | Add store + chrono deps |
| 2 | `inboxly-ui/src/feed.rs` | DateGroup enum + from_date() |
| 3 | `inboxly-ui/src/feed.rs` | FeedItem + FeedSection types |
| 4 | `inboxly-ui/src/feed.rs` | format_timestamp() |
| 5 | `inboxly-ui/src/feed.rs` | build_feed() query + grouping |
| 6 | `inboxly-store/src/lib.rs` | query_inbox_threads() + InboxThreadSummary |
| 7 | `inboxly-ui/src/widgets/avatar.rs` | Avatar circle (40dp, A-Z colours, canvas) |
| 8 | `inboxly-ui/src/widgets/section_header.rs` | SectionHeader (48dp, 14sp bold grey) |
| 9 | `inboxly-ui/src/widgets/email_row.rs` | EmailRow (avatar + sender + subject/snippet + timestamp + 📎) |
| 10 | `inboxly-ui/src/widgets/empty_state.rs` | Empty inbox placeholder |
| 11 | `inboxly-ui/src/views/inbox_view.rs` | Scrollable feed (sections + rows) |
| 12 | `inboxly-ui/src/app.rs` | Wire feed into main app shell |
| 13 | `inboxly/src/main.rs` | Wire Store into binary |
| 14 | `inboxly-ui/src/feed.rs` | Unit tests: DateGroup |
| 15 | `inboxly-ui/src/feed.rs` | Unit tests: timestamp formatting |
| 16 | `inboxly-ui/tests/feed_integration.rs` | Integration test: feed + store |
| 17 | (workspace) | Final clippy + test pass |

**Total commits:** 15 (Tasks 3+4 could merge, Tasks 14+15 could merge, but single-action commits are preferred for reviewability).

**After M17:** The inbox shows real emails on screen for the first time — threads grouped by date, with avatar circles, sender names, subjects, snippets, and timestamps. This is the "First Visual" checkpoint from the roadmap.
