# M27: Overflow + Context Menus — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan.

**Goal:** Wire the PopupMenu widget (built in M26) into the application UI: add a gear icon to the toolbar for Settings navigation, append a three-dot overflow menu to hover actions on email rows, and implement right-click context menus on email rows. Add the `ActiveView::Settings` variant with distinct toolbar colour and back-arrow navigation.

**Crates:** `inboxly-ui` (primary), `inboxly-store` (thread-action queries for MoveTo/MarkRead/Mute/BlockSender)

**Branch:** `m27-overflow-context-menus`

**Prereqs:** M26 complete (PopupMenu widget with `PopupMenu`, `MenuItem`, `MenuItemStyle`, `PopupAnchor` types and overlay rendering). M20 complete (hover actions: Done/Pin/Snooze buttons on email rows). M19 complete (MarkDone/TogglePin/Sweep messages and undo system).

**Spec ref:** Design spec Systems 2 + 3 (lines 87-141): Toolbar Integration, Three-Dot Overflow, Right-Click Context Menu.

---

## Gap Analysis (Plan vs. Codebase)

These are known divergences between the spec and the actual codebase that the implementation must account for:

1. **`ActiveView` enum** (`inboxly-ui/src/theme/mod.rs:49-54`) currently has `Inbox`, `Snoozed`, `Done`. Must add `Settings` variant. The `title()`, `toolbar_color()`, and `toolbar_color_themed()` methods must all gain a `Settings` arm.

2. **`ThemeColors`** (`inboxly-ui/src/theme/colors.rs:13-36`) has no `toolbar_settings` colour. Must add `toolbar_settings: Color` field (`#455a64` light, `#37474f` dark — neutral blue-grey). Both `light()` and `dark()` constructors need updating.

3. **`FeedItem`** (`inboxly-ui/src/feed.rs:79-104`) has no `sender_address` field. The overflow menu needs the sender address for "Block sender" and "Create rule from sender". The `InboxThreadSummary` already has `sender_address` — it just needs to be propagated through `build_feed()` into `FeedItem`.

4. **`hover_action_buttons()`** (`inboxly-ui/src/widgets/hover_actions.rs:16-37`) currently returns a hardcoded row of 3 buttons (Done/Pin/Snooze). Must add a 4th "More" button (⋮) that takes an on-press message.

5. **`Message` enum** (`inboxly-ui/src/app.rs:39-70`) has no overflow/context menu messages, no MoveTo/MarkRead/Mute/Reply/Forward/Block/Report messages. These must be added.

6. **`Inboxly` struct** (`inboxly-ui/src/app.rs:15-36`) has no state fields for tracking which overflow/context menu is open, or the cursor position for context menus. Must add `overflow_menu_thread_id: Option<String>`, `context_menu_thread_id: Option<String>`, `context_menu_position: iced::Point`, and `previous_view: ActiveView` (for back-navigation from Settings).

7. **Toolbar view** (`inboxly-ui/src/toolbar.rs:18-93`) has no gear icon between search and avatar. Must insert it. When `active_view == Settings`, the hamburger must become a back arrow (←) and the title must show "Settings".

8. **`email_row()` function** (`inboxly-ui/src/widgets/email_row.rs:25-114`) does not accept action callbacks. It is a pure display widget. The `inbox_view` function calls it without any interactivity for the row itself. The overflow menu trigger (⋮ button) will be added via the hover actions overlay, not inside `email_row` directly.

9. **Right-click handling**: Iced 0.14 does not provide a built-in right-click handler on standard widgets. The inbox view will need to use `iced::widget::mouse_area` (available in Iced 0.14) which supports `on_right_press` — or the email row must be wrapped in a custom event-intercepting widget. The `mouse_area` widget is the cleaner approach.

10. **Store actions**: The store has `set_thread_done()` and `set_thread_pinned()` but lacks `move_thread_to_folder()`, `mark_thread_read()`, `mute_thread()`, `block_sender()`, or `report_spam()`. These are stubs for now — the UI sends the messages and logs them; actual IMAP-side operations are out of scope for M27.

---

## Task Overview

| # | Task | Scope | Est. |
|---|------|-------|------|
| 1 | Add `toolbar_settings` to `ThemeColors` | theme | 10 min |
| 2 | Add `ActiveView::Settings` variant + back-navigation state | theme, app | 15 min |
| 3 | Add `sender_address` to `FeedItem` | feed | 5 min |
| 4 | Define thread action messages | app | 15 min |
| 5 | Add overflow/context menu state to `Inboxly` | app | 10 min |
| 6 | Add gear icon to toolbar + Settings toolbar mode | toolbar | 20 min |
| 7 | Add overflow (⋮) button to hover actions | hover_actions | 10 min |
| 8 | Build overflow menu items helper | new file | 20 min |
| 9 | Wire overflow menu into inbox view | inbox_view | 25 min |
| 10 | Handle right-click with `mouse_area` | inbox_view | 20 min |
| 11 | Build context menu items helper | overflow_menu | 10 min |
| 12 | Wire context menu popup into inbox view | inbox_view | 15 min |
| 13 | Handle thread action messages in `update()` | app | 20 min |
| 14 | Unit tests: ActiveView::Settings, toolbar colour, FeedItem | theme, feed | 15 min |
| 15 | Unit tests: message handling, menu state transitions | app | 20 min |
| 16 | Integration: full build + clippy + all tests | workspace | 10 min |

---

## Task 1 — Add `toolbar_settings` to `ThemeColors`

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme/colors.rs`

Add a `toolbar_settings` field to `ThemeColors`. The Settings toolbar uses a neutral blue-grey (`#455a64` light, `#37474f` dark) to visually distinguish it from inbox/snoozed/done views.

**Changes:**

Add field after `toolbar_text`:

```rust
/// Toolbar colour for Settings view (`#455a64` light, `#37474f` dark).
pub toolbar_settings: Color,
```

In `light()`:

```rust
toolbar_settings: hex("#455a64"),
```

In `dark()`:

```rust
toolbar_settings: hex("#37474f"),
```

**Tests to add:**

```rust
#[test]
fn light_theme_toolbar_settings() {
    assert_color_hex(ThemeColors::light().toolbar_settings, "#455a64");
}

#[test]
fn dark_theme_toolbar_settings() {
    assert_color_hex(ThemeColors::dark().toolbar_settings, "#37474f");
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui -- theme::colors && cargo clippy -p inboxly-ui -- -D warnings
```

**Commit:** `feat(ui): add toolbar_settings colour to ThemeColors`

---

## Task 2 — Add `ActiveView::Settings` variant + back-navigation state

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme/mod.rs`

Add `Settings` to the `ActiveView` enum and update all `match` arms.

**Changes to `ActiveView`:**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActiveView {
    #[default]
    Inbox,
    Snoozed,
    Done,
    /// Full-screen settings view. Toolbar turns grey, hamburger becomes back arrow.
    Settings,
}
```

**Update `title()`:**

```rust
Self::Settings => "Settings",
```

**Update `toolbar_color()`:**

```rust
Self::Settings => color_from_hex(0x45, 0x5a, 0x64), // #455a64
```

**Update `toolbar_color_themed()`:**

```rust
Self::Settings => theme.colors.toolbar_settings,
```

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs`

Add `previous_view` field to `Inboxly` for back-navigation from Settings:

```rust
/// View to return to when leaving Settings (back arrow).
pub previous_view: ActiveView,
```

Default to `ActiveView::Inbox` in `Default::default()`.

Add `NavigateToSettings` and `NavigateBack` messages (or handle via existing `Navigate` — see Task 4).

**Tests to add** (in `theme/mod.rs` tests):

```rust
#[test]
fn settings_title() {
    assert_eq!(ActiveView::Settings.title(), "Settings");
}

#[test]
fn settings_toolbar_is_grey() {
    let c = ActiveView::Settings.toolbar_color();
    assert!((c.r - 0x45 as f32 / 255.0).abs() < 0.01);
    assert!((c.g - 0x5a as f32 / 255.0).abs() < 0.01);
    assert!((c.b - 0x64 as f32 / 255.0).abs() < 0.01);
}

#[test]
fn settings_toolbar_themed() {
    let light = InboxlyTheme::light();
    let c = ActiveView::Settings.toolbar_color_themed(&light);
    let expected = ThemeColors::light().toolbar_settings;
    assert!((c.r - expected.r).abs() < 0.01);
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui && cargo clippy -p inboxly-ui -- -D warnings
```

**Commit:** `feat(ui): add ActiveView::Settings variant with grey toolbar and back-nav state`

---

## Task 3 — Add `sender_address` to `FeedItem`

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/feed.rs`

The overflow menu's "Block sender" and "Create rule from sender" actions need the sender's email address. `InboxThreadSummary` already has `sender_address` — just propagate it.

**Add field to `FeedItem`:**

```rust
/// Sender's email address (for block/rule actions).
pub sender_address: String,
```

**Update `build_feed()` where `FeedItem` is constructed** (around line 192):

```rust
sender_address: thread.sender_address.clone(),
// (move the existing sender_name fallback logic after this line
// since sender_address is now available)
sender_name: if thread.sender_name.is_empty() {
    thread.sender_address
} else {
    thread.sender_name
},
```

Note the borrow ordering: `sender_address` must be cloned before `sender_address` is potentially moved into `sender_name`.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui -- feed && cargo clippy -p inboxly-ui -- -D warnings
```

**Commit:** `feat(ui): propagate sender_address into FeedItem for menu actions`

---

## Task 4 — Define thread action messages

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs`

Add new `Message` variants for all actions reachable from the overflow and context menus.

**New variants:**

```rust
/// Open the overflow (⋮) menu for a specific thread.
OpenOverflowMenu(String),
/// Close the overflow menu.
CloseOverflowMenu,
/// Open the right-click context menu for a thread at a cursor position.
OpenContextMenu {
    thread_id: String,
    position: iced::Point,
},
/// Close the right-click context menu.
CloseContextMenu,

/// Navigate to Settings view (gear icon).
NavigateToSettings,
/// Navigate back from Settings to previous view.
NavigateBack,

// --- Thread actions (overflow + context menu) ---

/// Move thread to a folder (Inbox / Trash / Spam).
MoveTo {
    thread_id: String,
    destination: MoveDestination,
},
/// Mark thread as read or unread.
MarkReadState {
    thread_id: String,
    read: bool,
},
/// Mute a thread (suppress future notifications).
MuteThread(String),
/// Reply to a thread.
Reply(String),
/// Reply all to a thread.
ReplyAll(String),
/// Forward a thread.
Forward(String),
/// Add thread to a bundle category.
AddToBundle {
    thread_id: String,
    category: String,
},
/// Create a rule from the sender (stub — shows "Coming soon" toast).
CreateRuleFromSender(String),
/// Block the sender.
BlockSender {
    thread_id: String,
    sender_address: String,
},
/// Report thread as spam.
ReportSpam(String),
```

**New enum** (define above `Message` or in a submodule):

```rust
/// IMAP folder destinations for the "Move to..." action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveDestination {
    Inbox,
    Trash,
    Spam,
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

(Will not fully compile yet — `update()` match arms are incomplete. That is expected until Task 13.)

**Commit:** `feat(ui): define Message variants for overflow and context menu actions`

---

## Task 5 — Add overflow/context menu state to `Inboxly`

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs`

Add state fields to track which popup menus are open.

**New fields on `Inboxly`:**

```rust
/// Thread ID whose overflow (⋮) menu is currently open. `None` = closed.
pub overflow_menu_thread: Option<String>,
/// Thread ID whose right-click context menu is currently open. `None` = closed.
pub context_menu_thread: Option<String>,
/// Cursor position where the context menu was triggered.
pub context_menu_position: iced::Point,
/// The view to return to when navigating back from Settings.
pub previous_view: ActiveView,
```

**Update `Default` impl:**

```rust
overflow_menu_thread: None,
context_menu_thread: None,
context_menu_position: iced::Point::ORIGIN,
previous_view: ActiveView::Inbox,
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add overflow/context menu state fields to Inboxly`

---

## Task 6 — Add gear icon to toolbar + Settings toolbar mode

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/toolbar.rs`

Two changes:

### 6a: Gear icon between search bar and avatar

Insert a gear button (Unicode ⚙ U+2699) in the toolbar row, positioned between the search bar and the account avatar. When clicked, emits `Message::NavigateToSettings`.

```rust
// Gear icon -- navigates to Settings
let gear = button(text("\u{2699}").size(20.0).color(Color::WHITE))
    .on_press(Message::NavigateToSettings)
    .padding([8, 12])
    .style(move |_theme, _status| button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: Color::WHITE,
        border: Border::default(),
        ..Default::default()
    });
```

Insert `gear` into the `row!` macro between `search` and `Space::new()...Fill` and `avatar`:

```rust
let toolbar_row = row![
    hamburger_or_back,
    title,
    Space::new().width(Length::Fixed(DEFAULT_PADDING)),
    search,
    Space::new().width(Length::Fill),
    gear,
    avatar,
]
```

### 6b: Settings toolbar mode

When `app.active_view == ActiveView::Settings`:
- Replace hamburger (☰) with a back arrow (← U+2190) that emits `Message::NavigateBack`
- Title shows "Settings"
- Nav drawer is hidden (handled in `view()` — Task 13)

```rust
let (hamburger_or_back, hamburger_msg) = if app.active_view == ActiveView::Settings {
    ("\u{2190}", Message::NavigateBack)  // ← back arrow
} else {
    ("\u{2630}", Message::ToggleDrawer)  // ☰ hamburger
};

let hamburger = button(text(hamburger_or_back).size(20.0).color(Color::WHITE))
    .on_press(hamburger_msg)
    .padding([8, 12])
    .style(move |_theme, _status| button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: Color::WHITE,
        border: Border::default(),
        ..Default::default()
    });
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add gear icon to toolbar, back-arrow for Settings view`

---

## Task 7 — Add overflow (⋮) button to hover actions

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/hover_actions.rs`

Add a 4th button to the hover action row: the three-dot overflow trigger (⋮ U+22EE).

**Change `hover_action_buttons()` signature** to accept an additional `on_more: Message` parameter:

```rust
pub fn hover_action_buttons<'a, Message: 'a + Clone>(
    on_done: Message,
    on_pin: Message,
    on_snooze: Message,
    on_more: Message,
    accent_color: Color,
    surface_color: Color,
) -> Element<'a, Message> {
    let done_btn = action_button("\u{2713}", "Done", on_done, accent_color, surface_color);
    let pin_btn = action_button("\u{1F4CC}", "Pin", on_pin, accent_color, surface_color);
    let snooze_btn = action_button(
        "\u{1F552}",
        "Snooze",
        on_snooze,
        accent_color,
        surface_color,
    );
    let more_btn = action_button(
        "\u{22EE}",
        "More",
        on_more,
        accent_color,
        surface_color,
    );

    row![done_btn, pin_btn, snooze_btn, more_btn]
        .spacing(4.0)
        .padding([0.0, DEFAULT_PADDING])
        .into()
}
```

**Update test:**

```rust
#[test]
fn hover_buttons_accessible() {
    let _: Element<'_, &str> = hover_action_buttons(
        "done",
        "pin",
        "snooze",
        "more",
        Color::from_rgb(0.26, 0.52, 0.96),
        Color::WHITE,
    );
}
```

**Update all callers** — search for `hover_action_buttons(` and add the `on_more` argument. Currently the hover actions are likely rendered inside the swipe container or inbox view. Grep for call sites and fix each one.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui -- hover && cargo clippy -p inboxly-ui -- -D warnings
```

**Commit:** `feat(ui): add overflow (⋮) button to hover action row`

---

## Task 8 — Build overflow menu items helper

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/overflow_menu.rs` (new file)

Create a helper module that builds the `Vec<MenuItem<Message>>` for the overflow (⋮) menu and the context menu. These are separate functions because the context menu includes the quick-action group (Done/Pin/Snooze) at the top, while the overflow menu does not (those actions are already visible as hover buttons).

**Note:** This module uses the `PopupMenu`, `MenuItem`, `MenuItemStyle`, and `PopupAnchor` types from M26. Assume they live in `crate::widgets::popup_menu`.

```rust
//! Overflow and context menu item builders.
//!
//! Builds `MenuItem` vectors for the three-dot overflow menu
//! and the right-click context menu on email rows.

use crate::app::{Message, MoveDestination};
use crate::widgets::popup_menu::{MenuItem, MenuItemStyle};

/// Build menu items for the three-dot overflow menu on an email row.
///
/// Groups:
/// 1. Thread actions: Move to (submenu), Mark read/unread, Mute
/// 2. Reply actions: Reply, Reply All, Forward
/// 3. Organisation: Add to bundle (submenu), Create rule from sender
/// 4. Safety (destructive): Block sender, Report spam
pub fn overflow_menu_items(
    thread_id: &str,
    sender_address: &str,
    is_unread: bool,
    bundle_categories: &[String],
) -> Vec<MenuItem<Message>> {
    let tid = thread_id.to_owned();

    // Group 1: Thread actions
    let move_submenu = MenuItem::Submenu {
        label: "Move to\u{2026}".into(),
        icon: Some('\u{1F4E5}'), // inbox tray
        items: vec![
            MenuItem::Action {
                label: "Inbox".into(),
                icon: Some('\u{1F4E5}'),
                message: Message::MoveTo {
                    thread_id: tid.clone(),
                    destination: MoveDestination::Inbox,
                },
                style: MenuItemStyle::Normal,
            },
            MenuItem::Action {
                label: "Trash".into(),
                icon: Some('\u{1F5D1}'),
                message: Message::MoveTo {
                    thread_id: tid.clone(),
                    destination: MoveDestination::Trash,
                },
                style: MenuItemStyle::Normal,
            },
            MenuItem::Action {
                label: "Spam".into(),
                icon: Some('\u{26A0}'),
                message: Message::MoveTo {
                    thread_id: tid.clone(),
                    destination: MoveDestination::Spam,
                },
                style: MenuItemStyle::Normal,
            },
        ],
    };

    let mark_read_label = if is_unread {
        "Mark as read"
    } else {
        "Mark as unread"
    };
    let mark_read = MenuItem::Action {
        label: mark_read_label.into(),
        icon: Some(if is_unread { '\u{2709}' } else { '\u{2709}' }), // envelope
        message: Message::MarkReadState {
            thread_id: tid.clone(),
            read: is_unread, // if unread, mark read; if read, mark unread
        },
        style: MenuItemStyle::Normal,
    };

    let mute = MenuItem::Action {
        label: "Mute thread".into(),
        icon: Some('\u{1F507}'), // muted speaker
        message: Message::MuteThread(tid.clone()),
        style: MenuItemStyle::Normal,
    };

    // Group 2: Reply actions
    let reply = MenuItem::Action {
        label: "Reply".into(),
        icon: Some('\u{21A9}'), // leftwards arrow with hook
        message: Message::Reply(tid.clone()),
        style: MenuItemStyle::Normal,
    };
    let reply_all = MenuItem::Action {
        label: "Reply All".into(),
        icon: Some('\u{21A9}'),
        message: Message::ReplyAll(tid.clone()),
        style: MenuItemStyle::Normal,
    };
    let forward = MenuItem::Action {
        label: "Forward".into(),
        icon: Some('\u{21AA}'), // rightwards arrow with hook
        message: Message::Forward(tid.clone()),
        style: MenuItemStyle::Normal,
    };

    // Group 3: Organisation
    let bundle_items: Vec<MenuItem<Message>> = bundle_categories
        .iter()
        .map(|cat| MenuItem::Action {
            label: cat.clone(),
            icon: None,
            message: Message::AddToBundle {
                thread_id: tid.clone(),
                category: cat.clone(),
            },
            style: MenuItemStyle::Normal,
        })
        .collect();

    let add_to_bundle = MenuItem::Submenu {
        label: "Add to bundle\u{2026}".into(),
        icon: Some('\u{1F4E6}'), // package
        items: bundle_items,
    };

    let create_rule = MenuItem::Action {
        label: "Create rule from sender".into(),
        icon: Some('\u{2699}'), // gear
        message: Message::CreateRuleFromSender(sender_address.to_owned()),
        style: MenuItemStyle::Normal,
    };

    // Group 4: Safety (destructive)
    let block = MenuItem::Action {
        label: "Block sender".into(),
        icon: Some('\u{1F6AB}'), // no entry
        message: Message::BlockSender {
            thread_id: tid.clone(),
            sender_address: sender_address.to_owned(),
        },
        style: MenuItemStyle::Destructive,
    };
    let report_spam = MenuItem::Action {
        label: "Report spam".into(),
        icon: Some('\u{26A0}'), // warning
        message: Message::ReportSpam(tid),
        style: MenuItemStyle::Destructive,
    };

    vec![
        // Group 1
        move_submenu,
        mark_read,
        mute,
        MenuItem::Separator,
        // Group 2
        reply,
        reply_all,
        forward,
        MenuItem::Separator,
        // Group 3
        add_to_bundle,
        create_rule,
        MenuItem::Separator,
        // Group 4
        block,
        report_spam,
    ]
}

/// Build menu items for the right-click context menu.
///
/// Identical to overflow menu but prepended with quick-action group
/// (Done, Pin, Snooze) since those buttons are not visible without hover.
pub fn context_menu_items(
    thread_id: &str,
    sender_address: &str,
    is_unread: bool,
    bundle_categories: &[String],
) -> Vec<MenuItem<Message>> {
    let tid = thread_id.to_owned();

    // Quick action group (same as hover buttons)
    let done = MenuItem::Action {
        label: "Done".into(),
        icon: Some('\u{2713}'), // checkmark
        message: Message::MarkDone(tid.clone()),
        style: MenuItemStyle::Normal,
    };
    let pin = MenuItem::Action {
        label: "Pin".into(),
        icon: Some('\u{1F4CC}'), // pushpin
        message: Message::TogglePin(tid.clone()),
        style: MenuItemStyle::Normal,
    };
    let snooze = MenuItem::Action {
        label: "Snooze".into(),
        icon: Some('\u{1F552}'), // clock
        // Snooze from context menu: for now, snooze to default time.
        // Full snooze picker integration via context menu is deferred.
        // Emit a placeholder that the update handler can interpret.
        message: Message::SnoozeThread {
            thread_id: tid,
            until: chrono::Utc::now() + chrono::Duration::hours(3),
        },
        style: MenuItemStyle::Normal,
    };

    let mut items = vec![done, pin, snooze, MenuItem::Separator];

    // Append the same groups as the overflow menu
    items.extend(overflow_menu_items(
        thread_id,
        sender_address,
        is_unread,
        bundle_categories,
    ));

    items
}
```

**Register the module** in `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/mod.rs`:

```rust
pub mod overflow_menu;
```

**Tests:**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overflow_menu_has_four_groups() {
        let items = overflow_menu_items("t1", "test@example.com", true, &["Social".into()]);
        // Count separators — should be 3 (between 4 groups)
        let sep_count = items
            .iter()
            .filter(|i| matches!(i, MenuItem::Separator))
            .count();
        assert_eq!(sep_count, 3);
    }

    #[test]
    fn context_menu_prepends_quick_actions() {
        let items = context_menu_items("t1", "test@example.com", true, &[]);
        // First 3 items should be Done, Pin, Snooze, then Separator
        assert!(matches!(&items[0], MenuItem::Action { label, .. } if label == "Done"));
        assert!(matches!(&items[1], MenuItem::Action { label, .. } if label == "Pin"));
        assert!(matches!(&items[2], MenuItem::Action { label, .. } if label == "Snooze"));
        assert!(matches!(&items[3], MenuItem::Separator));
    }

    #[test]
    fn mark_read_toggles_label() {
        let unread_items = overflow_menu_items("t1", "a@b.com", true, &[]);
        let read_items = overflow_menu_items("t1", "a@b.com", false, &[]);

        let find_mark = |items: &[MenuItem<Message>]| {
            items.iter().find_map(|i| {
                if let MenuItem::Action { label, .. } = i {
                    if label.starts_with("Mark as") {
                        return Some(label.clone());
                    }
                }
                None
            })
        };

        assert_eq!(find_mark(&unread_items), Some("Mark as read".into()));
        assert_eq!(find_mark(&read_items), Some("Mark as unread".into()));
    }

    #[test]
    fn bundle_submenu_reflects_categories() {
        let cats = vec!["Social".into(), "Promos".into(), "Finance".into()];
        let items = overflow_menu_items("t1", "a@b.com", true, &cats);
        let bundle_sub = items.iter().find(|i| {
            matches!(i, MenuItem::Submenu { label, .. } if label.starts_with("Add to bundle"))
        });
        assert!(bundle_sub.is_some());
        if let Some(MenuItem::Submenu { items: sub_items, .. }) = bundle_sub {
            assert_eq!(sub_items.len(), 3);
        }
    }
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui -- overflow_menu && cargo clippy -p inboxly-ui -- -D warnings
```

**Commit:** `feat(ui): add overflow and context menu item builders`

---

## Task 9 — Wire overflow menu into inbox view

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/inbox_view.rs`

The overflow menu attaches to each email row. When the ⋮ button is pressed, `Message::OpenOverflowMenu(thread_id)` is sent. The app state sets `overflow_menu_thread = Some(thread_id)`. The inbox view then wraps the matching email row in a `PopupMenu` widget.

### Approach

The inbox view function needs access to:
1. The overflow menu thread ID (to know which row has an open popup)
2. The bundle category names (for the "Add to bundle" submenu)
3. Theme colours (already passed in)

**Expand `inbox_view()` signature:**

```rust
pub fn inbox_view<'a>(
    sections: &[FeedSection],
    primary_text_color: Color,
    secondary_text_color: Color,
    surface_color: Color,
    divider_color: Color,
    accent_color: Color,
    overflow_menu_thread: Option<&str>,
    bundle_categories: &[String],
) -> Element<'a, InboxViewMessage> {
```

**Expand `InboxViewMessage`** to wrap the app-level messages needed by the menu:

```rust
#[derive(Debug, Clone)]
pub enum InboxViewMessage {
    ToggleBundle(String),
    /// Hover action: Done
    Done(String),
    /// Hover action: Pin
    Pin(String),
    /// Hover action: Snooze (placeholder — opens snooze picker in future)
    Snooze(String),
    /// Hover action: Open overflow menu
    OpenOverflow(String),
    /// Close overflow menu
    CloseOverflow,
    /// Thread action from overflow/context menu (forwarded to app Message)
    ThreadAction(Message),
}
```

Wait — `InboxViewMessage` should not directly contain `Message` (circular dependency). Instead, define the thread action types in a shared location, or have `InboxViewMessage` variants that mirror the needed actions. The cleanest approach: have `InboxViewMessage` contain the action variants directly, and `app.rs` maps them to the corresponding `Message` variants in the `InboxView(msg)` match arm.

**Alternative approach (simpler):** Have the inbox view emit `Message` directly (not `InboxViewMessage`) and remove the intermediate enum. This simplifies wiring but couples the view to the app message type. Given that the inbox view is already tightly coupled to the app (it renders app state), this is acceptable.

**Recommended approach:** Keep `InboxViewMessage` but add variants that the `update()` function maps. This preserves separation.

```rust
#[derive(Debug, Clone)]
pub enum InboxViewMessage {
    ToggleBundle(String),
    HoverDone(String),
    HoverPin(String),
    HoverSnooze(String),
    OpenOverflow(String),
    CloseOverflow,
    // Forward thread actions as-is to app Message
    OverflowAction(OverflowAction),
}

/// Actions originating from overflow or context menus.
/// Mapped to app-level Message variants in update().
#[derive(Debug, Clone)]
pub enum OverflowAction {
    MoveTo { thread_id: String, destination: MoveDestination },
    MarkReadState { thread_id: String, read: bool },
    MuteThread(String),
    Reply(String),
    ReplyAll(String),
    Forward(String),
    AddToBundle { thread_id: String, category: String },
    CreateRuleFromSender(String),
    BlockSender { thread_id: String, sender_address: String },
    ReportSpam(String),
    MarkDone(String),
    TogglePin(String),
    SnoozeThread { thread_id: String, until: chrono::DateTime<chrono::Utc> },
}
```

Place `OverflowAction` and `MoveDestination` in a shared types module (e.g., `inboxly-ui/src/actions.rs`) that both `app.rs` and `inbox_view.rs` can import without circular deps.

**For each `FeedEntry::Thread(item)` in the view loop:**

1. Build the email row as before
2. Wrap it in a `mouse_area` (for right-click — Task 10)
3. If `overflow_menu_thread == Some(item.thread_id)`, wrap the row in a `PopupMenu` with `is_open: true`, anchor `BelowRight`, items from `overflow_menu_items()`

**Wiring the hover action buttons** — when building each email row, pass the `on_more` callback that emits `InboxViewMessage::OpenOverflow(thread_id.clone())`.

**Update `app.rs` InboxView match arm** to handle new `InboxViewMessage` variants and map `OverflowAction` to top-level `Message`.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): wire overflow popup menu to email rows in inbox view`

---

## Task 10 — Handle right-click with `mouse_area`

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/inbox_view.rs`

Iced 0.14 provides `iced::widget::mouse_area` which supports `on_right_press`. Wrap each email row in a `mouse_area` that, on right-click, emits `InboxViewMessage::OpenContextMenu { thread_id, position }`.

**Challenge:** `mouse_area::on_right_press` in Iced 0.14 takes a `Message` (not a closure with cursor position). To capture the cursor position, we have two options:

1. **Use `on_right_press` + subscription**: emit a fixed message on right-click, then use `iced::mouse::cursor_position()` or a subscription to get the cursor position. This is fragile.

2. **Use a thin custom widget wrapper**: intercept `Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right))` in a custom widget's `on_event()` method, read `cursor.position()`, and emit a message with the position. This is the robust approach.

3. **Check if Iced 0.14 `mouse_area` has `on_right_press_with`**: Some versions expose cursor position. If not available, use approach 2.

**Recommended: Approach 2 — custom `RightClickArea` widget.**

Create a minimal wrapper widget that passes through layout/draw to its child but intercepts right-click events:

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/right_click_area.rs` (new file)

```rust
//! Right-click interceptor widget.
//!
//! Wraps a child element and emits a message with cursor position
//! when the user right-clicks within the widget's bounds.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::event::Status;
use iced::mouse;
use iced::{Element, Event, Length, Point, Size};

pub struct RightClickArea<'a, Message> {
    content: Element<'a, Message>,
    on_right_click: Box<dyn Fn(Point) -> Message + 'a>,
}

impl<'a, Message: 'a> RightClickArea<'a, Message> {
    pub fn new(
        content: impl Into<Element<'a, Message>>,
        on_right_click: impl Fn(Point) -> Message + 'a,
    ) -> Self {
        Self {
            content: content.into(),
            on_right_click: Box::new(on_right_click),
        }
    }
}
```

Implement `Widget` trait: delegate `layout()`, `draw()`, `children()`, `diff()` to `content`. Override `on_event()` to check for `Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right))`, verify cursor is within bounds, and emit the message.

**Register module** in `widgets/mod.rs`:

```rust
pub mod right_click_area;
```

**Wire into inbox view:** Wrap each email row's `mouse_area` or container with `RightClickArea`:

```rust
let row_element = right_click_area::RightClickArea::new(
    email_row_widget,
    move |position| InboxViewMessage::OpenContextMenu {
        thread_id: tid.clone(),
        position,
    },
);
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add RightClickArea widget for context menu trigger`

---

## Task 11 — Build context menu items helper

This is already done in Task 8 — the `context_menu_items()` function builds the full context menu (quick actions + overflow actions). No additional code is needed.

**If not already tested**, add a test verifying the context menu has 4 separators (3 from overflow + 1 between quick-actions and overflow):

```rust
#[test]
fn context_menu_has_correct_separator_count() {
    let items = context_menu_items("t1", "a@b.com", true, &["Social".into()]);
    let sep_count = items
        .iter()
        .filter(|i| matches!(i, MenuItem::Separator))
        .count();
    assert_eq!(sep_count, 4); // quick|group1|group2|group3|group4
}
```

**Commit:** (no separate commit — covered by Task 8)

---

## Task 12 — Wire context menu popup into inbox view

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/inbox_view.rs`

When `context_menu_thread == Some(thread_id)`, render a `PopupMenu` with `PopupAnchor::AtCursor` at `context_menu_position`. The context menu is not attached to a specific row's overflow button — it is a free-floating overlay at the cursor position.

**Implementation:**

Since only one menu can be open at a time (overflow OR context, not both), the inbox view checks:

1. If `overflow_menu_thread` is `Some`, the PopupMenu is anchored to the ⋮ button of that row (handled in Task 9).
2. If `context_menu_thread` is `Some`, render a PopupMenu with `AtCursor` anchor at the stored position.

The PopupMenu for the context menu should be rendered as an overlay at the top level of the inbox view (not nested inside a specific row), using the cursor position stored in `context_menu_position`.

**Add `InboxViewMessage::OpenContextMenu` and `CloseContextMenu` variants:**

```rust
OpenContextMenu {
    thread_id: String,
    position: iced::Point,
},
CloseContextMenu,
```

**Expand `inbox_view()` signature** to also accept `context_menu_thread: Option<&str>` and `context_menu_position: iced::Point`.

**At the end of the view function**, if `context_menu_thread` is `Some`, wrap the entire scrollable in a `PopupMenu` with `AtCursor` anchor and the context menu items.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): wire right-click context menu popup into inbox view`

---

## Task 13 — Handle thread action messages in `update()`

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs`

Handle all new `Message` variants in `update()`. Most thread actions are stubs for now (log the action, show a toast via the undo snackbar where appropriate).

### NavigateToSettings:

```rust
Message::NavigateToSettings => {
    self.previous_view = self.active_view;
    self.active_view = ActiveView::Settings;
    self.drawer_open = false; // hide nav drawer in settings
}
```

### NavigateBack:

```rust
Message::NavigateBack => {
    self.active_view = self.previous_view;
    self.drawer_open = true; // restore nav drawer
}
```

### OpenOverflowMenu / CloseOverflowMenu:

```rust
Message::OpenOverflowMenu(thread_id) => {
    // Close any context menu first
    self.context_menu_thread = None;
    self.overflow_menu_thread = Some(thread_id);
}
Message::CloseOverflowMenu => {
    self.overflow_menu_thread = None;
}
```

### OpenContextMenu / CloseContextMenu:

```rust
Message::OpenContextMenu { thread_id, position } => {
    // Close any overflow menu first
    self.overflow_menu_thread = None;
    self.context_menu_thread = Some(thread_id);
    self.context_menu_position = position;
}
Message::CloseContextMenu => {
    self.context_menu_thread = None;
}
```

### Thread actions (stubs):

```rust
Message::MoveTo { thread_id, destination } => {
    tracing::info!("move thread {thread_id} to {destination:?}");
    // Actual IMAP move is out of scope for M27.
    // For Trash: could reuse MarkDone + a "trashed" flag.
    self.overflow_menu_thread = None;
    self.context_menu_thread = None;
}
Message::MarkReadState { thread_id, read } => {
    tracing::info!("mark thread {thread_id} read={read}");
    // Store integration: set unread flag on thread emails.
    // Stub for M27.
    self.overflow_menu_thread = None;
    self.context_menu_thread = None;
    self.reload_feed();
}
Message::MuteThread(thread_id) => {
    tracing::info!("mute thread {thread_id}");
    self.overflow_menu_thread = None;
    self.context_menu_thread = None;
}
Message::Reply(thread_id) => {
    tracing::info!("reply to thread {thread_id}");
    // Will open compose view in future milestone.
    self.overflow_menu_thread = None;
    self.context_menu_thread = None;
}
Message::ReplyAll(thread_id) => {
    tracing::info!("reply all to thread {thread_id}");
    self.overflow_menu_thread = None;
    self.context_menu_thread = None;
}
Message::Forward(thread_id) => {
    tracing::info!("forward thread {thread_id}");
    self.overflow_menu_thread = None;
    self.context_menu_thread = None;
}
Message::AddToBundle { thread_id, category } => {
    tracing::info!("add thread {thread_id} to bundle {category}");
    self.overflow_menu_thread = None;
    self.context_menu_thread = None;
}
Message::CreateRuleFromSender(sender) => {
    tracing::info!("create rule from sender: {sender} (coming soon)");
    // Show "Coming soon" toast via undo snackbar
    self.overflow_menu_thread = None;
    self.context_menu_thread = None;
}
Message::BlockSender { thread_id, sender_address } => {
    tracing::info!("block sender {sender_address} (thread {thread_id})");
    self.overflow_menu_thread = None;
    self.context_menu_thread = None;
}
Message::ReportSpam(thread_id) => {
    tracing::info!("report spam: thread {thread_id}");
    // Could move to Spam folder + mark done.
    self.overflow_menu_thread = None;
    self.context_menu_thread = None;
}
```

### Update `view()` to pass menu state to inbox_view:

```rust
let content_area: Element<Message> = if self.active_view == ActiveView::Inbox {
    let bundle_cats: Vec<String> = self
        .bundle_categories
        .iter()
        .map(|c| c.name.clone())
        .collect();

    inbox_view(
        &self.feed_sections,
        self.theme.colors.text_primary,
        self.theme.colors.text_secondary,
        self.theme.colors.surface,
        self.theme.colors.divider,
        self.theme.colors.toolbar_inbox, // accent_color
        self.overflow_menu_thread.as_deref(),
        self.context_menu_thread.as_deref(),
        self.context_menu_position,
        &bundle_cats,
    )
    .map(/* map InboxViewMessage to Message */)
} else if self.active_view == ActiveView::Settings {
    // Settings placeholder (actual settings UI is M29)
    container(
        text("Settings — coming in M29").size(16.0),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding(crate::theme::DEFAULT_PADDING)
    .into()
} else {
    // ... existing placeholder ...
};
```

### Update `view()` to hide nav drawer in Settings:

```rust
let drawer = if self.drawer_open && self.active_view != ActiveView::Settings {
    Some(view_drawer(self))
} else {
    None
};
```

### Map `InboxViewMessage` to `Message` in the existing match arm:

```rust
Message::InboxView(inbox_msg) => match inbox_msg {
    InboxViewMessage::ToggleBundle(bundle_id) => {
        tracing::debug!("toggle bundle: {bundle_id}");
    }
    InboxViewMessage::HoverDone(tid) => {
        return self.update(Message::MarkDone(tid));
    }
    InboxViewMessage::HoverPin(tid) => {
        return self.update(Message::TogglePin(tid));
    }
    InboxViewMessage::HoverSnooze(tid) => {
        // Open snooze picker (deferred, just log for now)
        tracing::debug!("hover snooze: {tid}");
    }
    InboxViewMessage::OpenOverflow(tid) => {
        return self.update(Message::OpenOverflowMenu(tid));
    }
    InboxViewMessage::CloseOverflow => {
        return self.update(Message::CloseOverflowMenu);
    }
    InboxViewMessage::OpenContextMenu { thread_id, position } => {
        return self.update(Message::OpenContextMenu { thread_id, position });
    }
    InboxViewMessage::CloseContextMenu => {
        return self.update(Message::CloseContextMenu);
    }
    InboxViewMessage::OverflowAction(action) => {
        // Map OverflowAction variants to top-level Message variants
        let msg = match action {
            OverflowAction::MoveTo { thread_id, destination } => {
                Message::MoveTo { thread_id, destination }
            }
            OverflowAction::MarkReadState { thread_id, read } => {
                Message::MarkReadState { thread_id, read }
            }
            OverflowAction::MuteThread(tid) => Message::MuteThread(tid),
            OverflowAction::Reply(tid) => Message::Reply(tid),
            OverflowAction::ReplyAll(tid) => Message::ReplyAll(tid),
            OverflowAction::Forward(tid) => Message::Forward(tid),
            OverflowAction::AddToBundle { thread_id, category } => {
                Message::AddToBundle { thread_id, category }
            }
            OverflowAction::CreateRuleFromSender(s) => Message::CreateRuleFromSender(s),
            OverflowAction::BlockSender { thread_id, sender_address } => {
                Message::BlockSender { thread_id, sender_address }
            }
            OverflowAction::ReportSpam(tid) => Message::ReportSpam(tid),
            OverflowAction::MarkDone(tid) => Message::MarkDone(tid),
            OverflowAction::TogglePin(tid) => Message::TogglePin(tid),
            OverflowAction::SnoozeThread { thread_id, until } => {
                Message::SnoozeThread { thread_id, until }
            }
        };
        return self.update(msg);
    }
},
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo build -p inboxly-ui && cargo clippy -p inboxly-ui -- -D warnings
```

**Commit:** `feat(ui): handle all overflow/context menu messages in app update()`

---

## Task 14 — Unit tests: ActiveView::Settings, toolbar colour, FeedItem

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme/colors.rs` (tests from Task 1)
**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/theme/mod.rs` (tests from Task 2)
**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs`

Add tests for:

### Settings navigation:

```rust
#[test]
fn navigate_to_settings_stores_previous_view() {
    let mut app = Inboxly::default();
    let _ = app.update(Message::Navigate(NavTarget::View(ActiveView::Snoozed)));
    let _ = app.update(Message::NavigateToSettings);
    assert_eq!(app.active_view, ActiveView::Settings);
    assert_eq!(app.previous_view, ActiveView::Snoozed);
    assert!(!app.drawer_open);
}

#[test]
fn navigate_back_from_settings_restores_view() {
    let mut app = Inboxly::default();
    let _ = app.update(Message::Navigate(NavTarget::View(ActiveView::Done)));
    let _ = app.update(Message::NavigateToSettings);
    let _ = app.update(Message::NavigateBack);
    assert_eq!(app.active_view, ActiveView::Done);
    assert!(app.drawer_open);
}
```

### Settings toolbar colour:

```rust
#[test]
fn settings_toolbar_distinct_from_all_views() {
    let settings_color = ActiveView::Settings.toolbar_color();
    assert_ne!(settings_color, ActiveView::Inbox.toolbar_color());
    assert_ne!(settings_color, ActiveView::Snoozed.toolbar_color());
    assert_ne!(settings_color, ActiveView::Done.toolbar_color());
}
```

### FeedItem sender_address:

```rust
#[test]
fn feed_item_has_sender_address() {
    let item = FeedItem {
        thread_id: "t1".into(),
        sender_name: "Alice".into(),
        sender_address: "alice@example.com".into(),
        avatar_letter: 'A',
        avatar_color_index: 0,
        subject: "Test".into(),
        snippet: "Hello".into(),
        timestamp: chrono::Utc::now(),
        timestamp_display: "Now".into(),
        is_unread: true,
        has_attachments: false,
        is_pinned: false,
        email_count: 1,
    };
    assert_eq!(item.sender_address, "alice@example.com");
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui && cargo clippy -p inboxly-ui -- -D warnings
```

**Commit:** `test(ui): add unit tests for Settings nav, toolbar colour, FeedItem sender_address`

---

## Task 15 — Unit tests: message handling, menu state transitions

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/app.rs`

### Overflow menu state transitions:

```rust
#[test]
fn open_overflow_menu_sets_state() {
    let mut app = Inboxly::default();
    let _ = app.update(Message::OpenOverflowMenu("t1".into()));
    assert_eq!(app.overflow_menu_thread, Some("t1".into()));
}

#[test]
fn close_overflow_menu_clears_state() {
    let mut app = Inboxly::default();
    let _ = app.update(Message::OpenOverflowMenu("t1".into()));
    let _ = app.update(Message::CloseOverflowMenu);
    assert!(app.overflow_menu_thread.is_none());
}

#[test]
fn open_context_menu_closes_overflow() {
    let mut app = Inboxly::default();
    let _ = app.update(Message::OpenOverflowMenu("t1".into()));
    let _ = app.update(Message::OpenContextMenu {
        thread_id: "t2".into(),
        position: iced::Point::new(100.0, 200.0),
    });
    assert!(app.overflow_menu_thread.is_none());
    assert_eq!(app.context_menu_thread, Some("t2".into()));
    assert_eq!(app.context_menu_position, iced::Point::new(100.0, 200.0));
}

#[test]
fn open_overflow_closes_context_menu() {
    let mut app = Inboxly::default();
    let _ = app.update(Message::OpenContextMenu {
        thread_id: "t1".into(),
        position: iced::Point::ORIGIN,
    });
    let _ = app.update(Message::OpenOverflowMenu("t2".into()));
    assert!(app.context_menu_thread.is_none());
    assert_eq!(app.overflow_menu_thread, Some("t2".into()));
}
```

### Thread action messages don't panic:

```rust
#[test]
fn thread_actions_do_not_panic() {
    let mut app = Inboxly::default();
    // All these should handle gracefully without a store
    let _ = app.update(Message::MoveTo {
        thread_id: "t1".into(),
        destination: MoveDestination::Trash,
    });
    let _ = app.update(Message::MarkReadState {
        thread_id: "t1".into(),
        read: true,
    });
    let _ = app.update(Message::MuteThread("t1".into()));
    let _ = app.update(Message::Reply("t1".into()));
    let _ = app.update(Message::ReplyAll("t1".into()));
    let _ = app.update(Message::Forward("t1".into()));
    let _ = app.update(Message::AddToBundle {
        thread_id: "t1".into(),
        category: "Social".into(),
    });
    let _ = app.update(Message::CreateRuleFromSender("a@b.com".into()));
    let _ = app.update(Message::BlockSender {
        thread_id: "t1".into(),
        sender_address: "a@b.com".into(),
    });
    let _ = app.update(Message::ReportSpam("t1".into()));
}

#[test]
fn thread_actions_close_menus() {
    let mut app = Inboxly::default();
    let _ = app.update(Message::OpenOverflowMenu("t1".into()));
    let _ = app.update(Message::MoveTo {
        thread_id: "t1".into(),
        destination: MoveDestination::Inbox,
    });
    assert!(app.overflow_menu_thread.is_none());
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui && cargo clippy -p inboxly-ui -- -D warnings
```

**Commit:** `test(ui): add tests for menu state transitions and thread action handling`

---

## Task 16 — Integration: full build + clippy + all tests

Run the complete workspace verification suite.

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo fmt --check --all && cargo clippy --workspace -- -D warnings && cargo test --workspace
```

Fix any remaining issues. Ensure:
- All existing tests still pass (no regressions from `ActiveView` match arm changes, `FeedItem` field addition, `hover_action_buttons` signature change)
- Zero clippy warnings
- Formatting is clean

**Commit:** `chore(ui): fix any remaining clippy/fmt issues from M27`

---

## New Files Summary

| File | Purpose |
|------|---------|
| `inboxly-ui/src/widgets/overflow_menu.rs` | Overflow + context menu item builders |
| `inboxly-ui/src/widgets/right_click_area.rs` | Custom widget for right-click event capture with cursor position |
| `inboxly-ui/src/actions.rs` | Shared `OverflowAction` and `MoveDestination` types (if extracted from app.rs) |

## Modified Files Summary

| File | Changes |
|------|---------|
| `inboxly-ui/src/theme/colors.rs` | Add `toolbar_settings` field + light/dark values + tests |
| `inboxly-ui/src/theme/mod.rs` | Add `ActiveView::Settings` + update `title()`/`toolbar_color()`/`toolbar_color_themed()` + tests |
| `inboxly-ui/src/feed.rs` | Add `sender_address` to `FeedItem`, update `build_feed()` |
| `inboxly-ui/src/app.rs` | New `Message` variants, menu state fields, `update()` handlers, `view()` wiring |
| `inboxly-ui/src/toolbar.rs` | Gear icon, back-arrow mode for Settings |
| `inboxly-ui/src/widgets/hover_actions.rs` | Add `on_more` parameter for ⋮ button |
| `inboxly-ui/src/widgets/mod.rs` | Register new modules |
| `inboxly-ui/src/views/inbox_view.rs` | Wire PopupMenu for overflow + context, expand `InboxViewMessage`, pass menu state |

## Risks and Mitigations

1. **Iced 0.14 overlay stacking**: If both overflow and context menus try to render simultaneously, overlay Z-ordering could conflict. **Mitigation**: Only one menu open at a time — opening one closes the other (enforced in `update()`).

2. **`RightClickArea` custom widget complexity**: Implementing a full Iced `Widget` trait is non-trivial. **Mitigation**: The widget is a thin pass-through — it delegates all methods to its child except `on_event()`. Follow existing custom widget patterns in `inboxly-ui` (e.g., swipe container if it exists as a full widget, or the PopupMenu from M26).

3. **`InboxViewMessage` explosion**: The message enum is growing large. **Mitigation**: Group related variants using nested enums (`OverflowAction`). Consider refactoring to a `ThreadAction` enum shared between views in a future milestone.

4. **Circular dependency risk**: If `overflow_menu.rs` imports from `app.rs` and `app.rs` imports from `overflow_menu.rs`, Rust will reject it. **Mitigation**: Extract shared types (`MoveDestination`, `OverflowAction`) into `actions.rs` which both modules import.

5. **`FeedItem` clone cost**: Adding `sender_address: String` increases clone cost for every feed item. **Mitigation**: This is negligible — feed items are already cloned infrequently (only on feed rebuild), and the string is typically < 50 bytes.
