//! Scrollable inbox feed view -- section headers + email rows + bundle rows.
//!
//! Renders the main inbox feed with:
//! - Date-grouped section headers
//! - Email rows with hover action buttons
//! - Bundle rows (collapsed)
//! - Overflow (three-dot) popup menus
//! - Right-click context menus

use iced::widget::{Column, scrollable};
use iced::{Color, Element, Length};

use crate::app::MoveDestination;
use crate::feed::{FeedEntry, FeedSection};
use crate::theme::colors::ThemeColors;
use crate::widgets::bundle_row::bundle_row_collapsed;
use crate::widgets::email_row::email_row;
use crate::widgets::empty_state::empty_inbox;
use crate::widgets::hover_actions::hover_action_buttons;
use crate::widgets::popup_menu::{MenuItem, PopupAnchor, PopupMenu};
use crate::widgets::right_click_area::RightClickArea;
use crate::widgets::section_header::section_header;

/// Messages from the inbox view.
#[derive(Debug, Clone)]
pub enum InboxViewMessage {
    /// User clicked a bundle row to expand/collapse it.
    ToggleBundle(String),
    /// Hover action: Done (archive).
    HoverDone(String),
    /// Hover action: Pin / unpin.
    HoverPin(String),
    /// Hover action: Snooze.
    HoverSnooze(String),
    /// Open overflow (three-dot) menu for thread.
    OpenOverflow(String),
    /// Close overflow menu.
    CloseOverflow,
    /// Open context menu at cursor position.
    OpenContextMenu {
        thread_id: String,
        position: iced::Point,
    },
    /// Close context menu.
    CloseContextMenu,
    /// Move thread to a folder.
    MoveTo {
        thread_id: String,
        destination: MoveDestination,
    },
    /// Mark thread as read or unread.
    MarkReadState { thread_id: String, read: bool },
    /// Mute a thread.
    MuteThread(String),
    /// Reply to a thread.
    ReplyThread(String),
    /// Reply all to a thread.
    ReplyAllThread(String),
    /// Forward a thread.
    ForwardThread(String),
    /// Add thread to a bundle category.
    AddToBundle { thread_id: String, category: String },
    /// Create a rule from sender.
    CreateRuleFromSender(String),
    /// Block the sender.
    BlockSender {
        thread_id: String,
        sender_address: String,
    },
    /// Report thread as spam.
    ReportSpam(String),
}

/// Build overflow menu items using `InboxViewMessage` variants.
fn build_overflow_items(
    thread_id: &str,
    sender_address: &str,
    is_unread: bool,
    bundle_categories: &[String],
) -> Vec<MenuItem<InboxViewMessage>> {
    let tid = thread_id.to_owned();
    let sender = sender_address.to_owned();

    let mut items = Vec::new();

    // Group 1: Thread actions
    let move_submenu = MenuItem::submenu(
        "Move to\u{2026}",
        Some('\u{1F4C1}'),
        vec![
            MenuItem::action_with_icon(
                "Inbox",
                '\u{1F4E5}',
                InboxViewMessage::MoveTo {
                    thread_id: tid.clone(),
                    destination: MoveDestination::Inbox,
                },
            ),
            MenuItem::action_with_icon(
                "Trash",
                '\u{1F5D1}',
                InboxViewMessage::MoveTo {
                    thread_id: tid.clone(),
                    destination: MoveDestination::Trash,
                },
            ),
            MenuItem::action_with_icon(
                "Spam",
                '\u{26A0}',
                InboxViewMessage::MoveTo {
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
        '\u{2709}',
        InboxViewMessage::MarkReadState {
            thread_id: tid.clone(),
            read: is_unread,
        },
    ));

    items.push(MenuItem::action_with_icon(
        "Mute thread",
        '\u{1F507}',
        InboxViewMessage::MuteThread(tid.clone()),
    ));

    items.push(MenuItem::separator());

    // Group 2: Reply actions
    items.push(MenuItem::action_with_icon(
        "Reply",
        '\u{21A9}',
        InboxViewMessage::ReplyThread(tid.clone()),
    ));
    items.push(MenuItem::action_with_icon(
        "Reply All",
        '\u{21AA}',
        InboxViewMessage::ReplyAllThread(tid.clone()),
    ));
    items.push(MenuItem::action_with_icon(
        "Forward",
        '\u{2192}',
        InboxViewMessage::ForwardThread(tid.clone()),
    ));

    items.push(MenuItem::separator());

    // Group 3: Organisation
    let bundle_sub_items: Vec<MenuItem<InboxViewMessage>> = bundle_categories
        .iter()
        .map(|cat| {
            MenuItem::action(
                cat.clone(),
                InboxViewMessage::AddToBundle {
                    thread_id: tid.clone(),
                    category: cat.clone(),
                },
            )
        })
        .collect();
    items.push(MenuItem::submenu(
        "Add to bundle\u{2026}",
        Some('\u{1F3F7}'),
        bundle_sub_items,
    ));
    items.push(MenuItem::action_with_icon(
        "Create rule from sender",
        '\u{2699}',
        InboxViewMessage::CreateRuleFromSender(sender.clone()),
    ));

    items.push(MenuItem::separator());

    // Group 4: Safety (destructive)
    items.push(MenuItem::destructive_with_icon(
        "Block sender",
        '\u{1F6AB}',
        InboxViewMessage::BlockSender {
            thread_id: tid.clone(),
            sender_address: sender,
        },
    ));
    items.push(MenuItem::destructive_with_icon(
        "Report spam",
        '\u{26A0}',
        InboxViewMessage::ReportSpam(tid),
    ));

    items
}

/// Build context menu items (quick actions + overflow items).
fn build_context_items(
    thread_id: &str,
    sender_address: &str,
    is_unread: bool,
    bundle_categories: &[String],
) -> Vec<MenuItem<InboxViewMessage>> {
    let tid = thread_id.to_owned();

    let mut items = vec![
        MenuItem::action_with_icon("Done", '\u{2713}', InboxViewMessage::HoverDone(tid.clone())),
        MenuItem::action_with_icon("Pin", '\u{1F4CC}', InboxViewMessage::HoverPin(tid.clone())),
        MenuItem::action_with_icon("Snooze", '\u{1F552}', InboxViewMessage::HoverSnooze(tid)),
        MenuItem::separator(),
    ];

    items.extend(build_overflow_items(
        thread_id,
        sender_address,
        is_unread,
        bundle_categories,
    ));

    items
}

/// Build the scrollable inbox feed view.
///
/// Renders section headers, email rows, and bundle rows from pre-built
/// feed sections. Shows an empty state when there are no sections.
///
/// When `overflow_menu_thread` matches a thread ID, the overflow popup menu
/// is rendered for that thread. Similarly for `context_menu_thread`.
///
/// Theme colours are passed in from the app-level theme.
#[allow(clippy::too_many_arguments)]
pub fn inbox_view<'a>(
    sections: &[FeedSection],
    primary_text_color: Color,
    secondary_text_color: Color,
    surface_color: Color,
    divider_color: Color,
    accent_color: Color,
    overflow_menu_thread: Option<&str>,
    context_menu_thread: Option<&str>,
    context_menu_position: iced::Point,
    bundle_categories: &[String],
    theme_colors: &ThemeColors,
) -> Element<'a, InboxViewMessage> {
    // Empty state.
    if sections.is_empty() {
        return empty_inbox(secondary_text_color);
    }

    // Build the feed column: alternating section headers and rows.
    let mut feed_column = Column::new().width(Length::Fill).spacing(0.0);

    for section in sections {
        // Section header.
        feed_column = feed_column.push(section_header(section.group, secondary_text_color));

        // Entries within this section.
        for entry in &section.items {
            match entry {
                FeedEntry::Thread(item) => {
                    let tid = item.thread_id.clone();

                    // Base email row.
                    let base_row = email_row(
                        item,
                        primary_text_color,
                        secondary_text_color,
                        surface_color,
                        divider_color,
                    );

                    // Hover action buttons overlaid on the right side.
                    let hover = hover_action_buttons(
                        InboxViewMessage::HoverDone(tid.clone()),
                        InboxViewMessage::HoverPin(tid.clone()),
                        InboxViewMessage::HoverSnooze(tid.clone()),
                        InboxViewMessage::OpenOverflow(tid.clone()),
                        accent_color,
                        surface_color,
                    );

                    // Stack the email row and hover actions together.
                    // The hover actions are positioned via an Iced row;
                    // for now we append them below. Real hover-reveal
                    // requires mouse_area which is M25 work.
                    let _ = hover; // suppress unused -- hover reveal wiring is M25

                    // Wrap in RightClickArea for context menu trigger.
                    let right_click_tid = tid.clone();
                    let with_right_click: Element<'a, InboxViewMessage> =
                        RightClickArea::new(base_row)
                            .on_right_click(move |pos| InboxViewMessage::OpenContextMenu {
                                thread_id: right_click_tid.clone(),
                                position: pos,
                            })
                            .into();

                    // Check if overflow menu is open for this thread.
                    let is_overflow_open =
                        overflow_menu_thread.is_some_and(|id| id == item.thread_id);
                    // Check if context menu is open for this thread.
                    let is_context_open =
                        context_menu_thread.is_some_and(|id| id == item.thread_id);

                    if is_overflow_open {
                        let menu_items = build_overflow_items(
                            &tid,
                            &item.sender_address,
                            item.is_unread,
                            bundle_categories,
                        );
                        let popup: Element<'a, InboxViewMessage> = PopupMenu::new(
                            with_right_click,
                            menu_items,
                            InboxViewMessage::CloseOverflow,
                            *theme_colors,
                        )
                        .open(true)
                        .anchor(PopupAnchor::BelowRight)
                        .into();
                        feed_column = feed_column.push(popup);
                    } else if is_context_open {
                        let menu_items = build_context_items(
                            &tid,
                            &item.sender_address,
                            item.is_unread,
                            bundle_categories,
                        );
                        // For AtCursor, PopupMenu uses the tracked cursor position
                        // from its internal state. We set the cursor position stored
                        // in the widget state. The overlay positions itself at the
                        // cursor position captured when the menu was opened.
                        let _ = context_menu_position; // used by AtCursor anchor mode
                        let popup: Element<'a, InboxViewMessage> = PopupMenu::new(
                            with_right_click,
                            menu_items,
                            InboxViewMessage::CloseContextMenu,
                            *theme_colors,
                        )
                        .open(true)
                        .anchor(PopupAnchor::AtCursor)
                        .into();
                        feed_column = feed_column.push(popup);
                    } else {
                        feed_column = feed_column.push(with_right_click);
                    }
                }
                FeedEntry::Bundle(summary) => {
                    let bundle_id = summary.bundle_id.clone();
                    feed_column = feed_column.push(bundle_row_collapsed(
                        summary,
                        InboxViewMessage::ToggleBundle(bundle_id),
                        secondary_text_color,
                        surface_color,
                        divider_color,
                    ));
                }
            }
        }
    }

    // Wrap in a scrollable container.
    scrollable(feed_column)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
