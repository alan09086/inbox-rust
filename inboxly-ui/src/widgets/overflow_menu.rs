//! Overflow and context menu item builders.
//!
//! Provides two builder functions that produce `Vec<MenuItem<Message>>`:
//! - `overflow_menu_items()` -- items for the three-dot overflow menu
//! - `context_menu_items()` -- items for the right-click context menu
//!   (prepends quick actions then includes all overflow items)

use crate::app::{Message, MoveDestination};
use crate::widgets::popup_menu::MenuItem;

/// Build the overflow (three-dot) menu items for a thread.
///
/// Returns items in 4 groups separated by `MenuItem::Separator`:
/// 1. Thread actions: Move to... (submenu), Mark as read/unread, Mute thread
/// 2. Reply actions: Reply, Reply All, Forward
/// 3. Organisation: Add to bundle... (submenu), Create rule from sender
/// 4. Safety (destructive): Block sender, Report spam
pub fn overflow_menu_items(
    thread_id: &str,
    sender_address: &str,
    is_unread: bool,
    bundle_categories: &[String],
) -> Vec<MenuItem<Message>> {
    let tid = thread_id.to_owned();
    let sender = sender_address.to_owned();

    let mut items = Vec::new();

    // Group 1: Thread actions
    let move_submenu = MenuItem::submenu(
        "Move to\u{2026}",
        Some('\u{1F4C1}'), // 📁
        vec![
            MenuItem::action_with_icon(
                "Inbox",
                '\u{1F4E5}', // 📥
                Message::MoveTo {
                    thread_id: tid.clone(),
                    destination: MoveDestination::Inbox,
                },
            ),
            MenuItem::action_with_icon(
                "Trash",
                '\u{1F5D1}', // 🗑
                Message::MoveTo {
                    thread_id: tid.clone(),
                    destination: MoveDestination::Trash,
                },
            ),
            MenuItem::action_with_icon(
                "Spam",
                '\u{26A0}', // ⚠
                Message::MoveTo {
                    thread_id: tid.clone(),
                    destination: MoveDestination::Spam,
                },
            ),
        ],
    );
    items.push(move_submenu);

    let mark_label = if is_unread {
        "Mark as read"
    } else {
        "Mark as unread"
    };
    items.push(MenuItem::action_with_icon(
        mark_label,
        '\u{2709}', // ✉
        Message::MarkReadState {
            thread_id: tid.clone(),
            read: is_unread, // if unread, mark as read; if read, mark as unread
        },
    ));

    items.push(MenuItem::action_with_icon(
        "Mute thread",
        '\u{1F507}', // 🔇
        Message::MuteThread(tid.clone()),
    ));

    // Separator 1
    items.push(MenuItem::separator());

    // Group 2: Reply actions
    items.push(MenuItem::action_with_icon(
        "Reply",
        '\u{21A9}', // ↩
        Message::Reply(tid.clone()),
    ));
    items.push(MenuItem::action_with_icon(
        "Reply All",
        '\u{21AA}', // ↪
        Message::ReplyAll(tid.clone()),
    ));
    items.push(MenuItem::action_with_icon(
        "Forward",
        '\u{2192}', // →
        Message::Forward(tid.clone()),
    ));

    // Separator 2
    items.push(MenuItem::separator());

    // Group 3: Organisation
    let bundle_sub_items: Vec<MenuItem<Message>> = bundle_categories
        .iter()
        .map(|cat| {
            MenuItem::action(
                cat.clone(),
                Message::AddToBundle {
                    thread_id: tid.clone(),
                    category: cat.clone(),
                },
            )
        })
        .collect();
    items.push(MenuItem::submenu(
        "Add to bundle\u{2026}",
        Some('\u{1F3F7}'), // 🏷
        bundle_sub_items,
    ));
    items.push(MenuItem::action_with_icon(
        "Create rule from sender",
        '\u{2699}', // ⚙
        Message::CreateRuleFromSender(sender.clone()),
    ));

    // Separator 3
    items.push(MenuItem::separator());

    // Group 4: Safety (destructive)
    items.push(MenuItem::destructive_with_icon(
        "Block sender",
        '\u{1F6AB}', // 🚫
        Message::BlockSender {
            thread_id: tid.clone(),
            sender_address: sender,
        },
    ));
    items.push(MenuItem::destructive_with_icon(
        "Report spam",
        '\u{26A0}', // ⚠
        Message::ReportSpam(tid),
    ));

    items
}

/// Build the right-click context menu items for a thread.
///
/// Prepends Done/Pin/Snooze quick actions + separator, then appends
/// all overflow menu items.
pub fn context_menu_items(
    thread_id: &str,
    sender_address: &str,
    is_unread: bool,
    bundle_categories: &[String],
) -> Vec<MenuItem<Message>> {
    let tid = thread_id.to_owned();

    // Quick actions + separator
    let mut items = vec![
        MenuItem::action_with_icon("Done", '\u{2713}', Message::MarkDone(tid.clone())),
        MenuItem::action_with_icon("Pin", '\u{1F4CC}', Message::TogglePin(tid.clone())),
        MenuItem::action_with_icon(
            "Snooze",
            '\u{1F552}',
            Message::SnoozeThread {
                thread_id: tid,
                until: chrono::Utc::now() + chrono::Duration::hours(3),
            },
        ),
        MenuItem::separator(),
    ];

    // Append all overflow menu items
    items.extend(overflow_menu_items(
        thread_id,
        sender_address,
        is_unread,
        bundle_categories,
    ));

    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overflow_menu_has_three_separators() {
        let items = overflow_menu_items("t1", "test@example.com", true, &["Social".into()]);
        let sep_count = items.iter().filter(|i| i.is_separator()).count();
        assert_eq!(sep_count, 3);
    }

    #[test]
    fn context_menu_prepends_quick_actions() {
        let items = context_menu_items("t1", "test@example.com", true, &[]);
        match &items[0] {
            MenuItem::Action { label, .. } => assert_eq!(label, "Done"),
            _ => panic!("expected Done action"),
        }
        match &items[1] {
            MenuItem::Action { label, .. } => assert_eq!(label, "Pin"),
            _ => panic!("expected Pin action"),
        }
        match &items[2] {
            MenuItem::Action { label, .. } => assert_eq!(label, "Snooze"),
            _ => panic!("expected Snooze action"),
        }
        assert!(items[3].is_separator());
    }

    #[test]
    fn mark_read_toggles_label() {
        let unread = overflow_menu_items("t1", "a@b.com", true, &[]);
        let read = overflow_menu_items("t1", "a@b.com", false, &[]);
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
        assert_eq!(find_mark(&unread), Some("Mark as read".into()));
        assert_eq!(find_mark(&read), Some("Mark as unread".into()));
    }

    #[test]
    fn bundle_submenu_reflects_categories() {
        let cats = vec!["Social".into(), "Promos".into()];
        let items = overflow_menu_items("t1", "a@b.com", true, &cats);
        let bundle_sub = items.iter().find(
            |i| matches!(i, MenuItem::Submenu { label, .. } if label.starts_with("Add to bundle")),
        );
        assert!(bundle_sub.is_some());
        if let Some(MenuItem::Submenu { items: sub, .. }) = bundle_sub {
            assert_eq!(sub.len(), 2);
        }
    }

    #[test]
    fn context_menu_has_four_separators() {
        let items = context_menu_items("t1", "a@b.com", true, &["Social".into()]);
        let sep_count = items.iter().filter(|i| i.is_separator()).count();
        assert_eq!(sep_count, 4);
    }

    #[test]
    fn destructive_items_are_last_group() {
        let items = overflow_menu_items("t1", "a@b.com", true, &[]);
        let last_sep_idx = items.iter().rposition(|i| i.is_separator()).unwrap();
        for item in &items[last_sep_idx + 1..] {
            assert!(
                item.is_destructive(),
                "items after last separator should be destructive"
            );
        }
    }
}
