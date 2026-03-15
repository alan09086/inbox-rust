//! Core application state and Iced elm-architecture implementation.

use iced::widget::{column, container, row, text};
use iced::{Element, Length, Task, Theme};

use inboxly_store::Store;

use crate::feed::{self, FeedSection};
use crate::nav::{NavBundleCategory, NavTarget, default_bundle_categories};
use crate::theme::{ActiveView, InboxlyTheme};
use crate::undo::{UndoAction, UndoState};
use crate::views::inbox_view::{InboxViewMessage, inbox_view};

/// Top-level application state.
pub struct Inboxly {
    /// Currently active primary view (drives toolbar colour).
    pub active_view: ActiveView,
    /// View to return to when leaving Settings (back arrow).
    pub previous_view: ActiveView,
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
    /// Active theme (light or dark, with full BigTop tokens).
    pub theme: InboxlyTheme,
    /// SQLite store for querying threads (None until wired from binary).
    pub store: Option<Store>,
    /// Pre-built feed sections for the inbox view.
    pub feed_sections: Vec<FeedSection>,
    /// Undo state for timed undo of inbox actions.
    pub undo_state: UndoState,
    /// Thread ID whose overflow (three-dot) menu is currently open.
    pub overflow_menu_thread: Option<String>,
    /// Thread ID whose right-click context menu is currently open.
    pub context_menu_thread: Option<String>,
    /// Cursor position where the context menu was triggered.
    pub context_menu_position: iced::Point,
}

/// IMAP folder destinations for the "Move to..." action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveDestination {
    Inbox,
    Trash,
    Spam,
}

/// All messages the application can receive.
// ThemeChanged carries InboxlyTheme which has grown with additional Color fields;
// boxing would change the API surface and is heavy-handed for a UI message enum.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum Message {
    /// User clicked a nav item.
    Navigate(NavTarget),
    /// User toggled the hamburger menu.
    ToggleDrawer,
    /// Search bar text changed (placeholder for now).
    SearchChanged(String),
    /// User toggled the theme (light <-> dark).
    ThemeToggled,
    /// Async system theme detection completed.
    ThemeChanged(InboxlyTheme),
    /// Reload the inbox feed from the store.
    ReloadFeed,
    /// Message from the inbox view (bundle toggle, etc.).
    InboxView(InboxViewMessage),
    /// Mark a thread as Done (archive).
    MarkDone(String),
    /// Toggle pin state for a thread.
    TogglePin(String),
    /// Sweep: mark all unpinned threads in the current section as Done.
    Sweep,
    /// User pressed Undo on the snackbar.
    Undo,
    /// Undo timer expired -- commit the action.
    UndoExpired,
    /// Snooze a thread until the given UTC timestamp.
    SnoozeThread {
        thread_id: String,
        until: chrono::DateTime<chrono::Utc>,
    },
    /// Open the overflow (three-dot) menu for a specific thread.
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
    Reply(String),
    /// Reply all to a thread.
    ReplyAll(String),
    /// Forward a thread.
    Forward(String),
    /// Add thread to a bundle category.
    AddToBundle { thread_id: String, category: String },
    /// Create a rule from sender (stub -- shows "Coming soon" toast).
    CreateRuleFromSender(String),
    /// Block the sender.
    BlockSender {
        thread_id: String,
        sender_address: String,
    },
    /// Report thread as spam.
    ReportSpam(String),
}

impl Default for Inboxly {
    fn default() -> Self {
        Self {
            active_view: ActiveView::Inbox,
            previous_view: ActiveView::Inbox,
            active_nav: NavTarget::View(ActiveView::Inbox),
            drawer_open: true,
            bundle_categories: default_bundle_categories(),
            account_email: "user@example.com".into(),
            account_count: 1,
            theme: InboxlyTheme::light(),
            store: None,
            feed_sections: Vec::new(),
            undo_state: UndoState::new(),
            overflow_menu_thread: None,
            context_menu_thread: None,
            context_menu_position: iced::Point::ORIGIN,
        }
    }
}

impl Inboxly {
    /// Create the app with initial state. Returns (Self, startup Task).
    ///
    /// Theme should be resolved before calling this (via
    /// `InboxlyTheme::from_system()`) since zbus requires Tokio
    /// and Iced doesn't provide one.
    pub fn new() -> (Self, Task<Message>) {
        let mut app = Self {
            theme: InboxlyTheme::from_system(),
            ..Self::default()
        };
        app.reload_feed();
        (app, Task::none())
    }

    /// Create the app with a store instance (called from binary crate).
    pub fn with_store(store: Store) -> (Self, Task<Message>) {
        let mut app = Self {
            store: Some(store),
            theme: InboxlyTheme::from_system(),
            ..Self::default()
        };
        app.reload_feed();
        (app, Task::none())
    }

    /// Iced update function -- handle messages and mutate state.
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Navigate(target) => {
                if let NavTarget::View(view) = &target {
                    self.active_view = *view;
                }
                self.active_nav = target;
            }
            Message::ToggleDrawer => {
                self.drawer_open = !self.drawer_open;
            }
            Message::SearchChanged(_query) => {
                // Placeholder -- search is M24.
            }
            Message::ThemeToggled => {
                self.theme = self.theme.toggle();
            }
            Message::ThemeChanged(new_theme) => {
                self.theme = new_theme;
            }
            Message::ReloadFeed => {
                self.reload_feed();
            }
            Message::InboxView(inbox_msg) => match inbox_msg {
                InboxViewMessage::ToggleBundle(bundle_id) => {
                    tracing::debug!("toggle bundle: {bundle_id}");
                }
            },
            Message::MarkDone(thread_id) => {
                if let Some(ref store) = self.store {
                    if let Err(e) = store.get_or_create_thread_state(&thread_id) {
                        tracing::warn!("failed to ensure thread state: {e}");
                    }
                    if let Err(e) = store.set_thread_done(&thread_id, true) {
                        tracing::warn!("failed to mark done: {e}");
                    }
                }
                self.undo_state.push(UndoAction::MarkDone { thread_id });
                self.reload_feed();
            }
            Message::TogglePin(thread_id) => {
                let was_pinned = self
                    .store
                    .as_ref()
                    .and_then(|store| store.get_thread_state(&thread_id).ok().map(|s| s.pinned));
                let was_pinned = was_pinned.unwrap_or(false);

                if let Some(ref store) = self.store {
                    if let Err(e) = store.get_or_create_thread_state(&thread_id) {
                        tracing::warn!("failed to ensure thread state: {e}");
                    }
                    if let Err(e) = store.set_thread_pinned(&thread_id, !was_pinned) {
                        tracing::warn!("failed to toggle pin: {e}");
                    }
                }
                self.undo_state.push(UndoAction::TogglePin {
                    thread_id,
                    was_pinned,
                });
                self.reload_feed();
            }
            Message::Sweep => {
                // Mark all unpinned, non-done threads as done.
                let mut swept = Vec::new();
                if let Some(ref store) = self.store
                    && let Ok(threads) = store.query_inbox_threads()
                {
                    for thread in threads {
                        if !thread.pinned {
                            if let Err(e) = store.get_or_create_thread_state(&thread.id) {
                                tracing::warn!("sweep: failed to ensure state: {e}");
                                continue;
                            }
                            if let Err(e) = store.set_thread_done(&thread.id, true) {
                                tracing::warn!("sweep: failed to mark done: {e}");
                                continue;
                            }
                            swept.push(thread.id);
                        }
                    }
                }
                if !swept.is_empty() {
                    self.undo_state
                        .push(UndoAction::Sweep { thread_ids: swept });
                }
                self.reload_feed();
            }
            Message::Undo => {
                if let Some(action) = self.undo_state.take() {
                    if let Some(ref store) = self.store {
                        match action {
                            UndoAction::MarkDone { thread_id } => {
                                let _ = store.set_thread_done(&thread_id, false);
                            }
                            UndoAction::TogglePin {
                                thread_id,
                                was_pinned,
                            } => {
                                let _ = store.set_thread_pinned(&thread_id, was_pinned);
                            }
                            UndoAction::Sweep { thread_ids } => {
                                for tid in &thread_ids {
                                    let _ = store.set_thread_done(tid, false);
                                }
                            }
                        }
                    }
                    self.reload_feed();
                }
            }
            Message::UndoExpired => {
                self.undo_state.clear();
            }
            Message::SnoozeThread { thread_id, until } => {
                if let Some(ref store) = self.store {
                    if let Err(e) = store.get_or_create_thread_state(&thread_id) {
                        tracing::warn!("failed to ensure thread state for snooze: {e}");
                    }
                    if let Err(e) =
                        store.set_thread_snoozed(&thread_id, Some(until.timestamp()), None)
                    {
                        tracing::warn!("failed to snooze thread: {e}");
                    }
                }
                self.reload_feed();
            }
            Message::OpenOverflowMenu(thread_id) => {
                self.context_menu_thread = None;
                self.overflow_menu_thread = Some(thread_id);
            }
            Message::CloseOverflowMenu => {
                self.overflow_menu_thread = None;
            }
            Message::OpenContextMenu {
                thread_id,
                position,
            } => {
                self.overflow_menu_thread = None;
                self.context_menu_thread = Some(thread_id);
                self.context_menu_position = position;
            }
            Message::CloseContextMenu => {
                self.context_menu_thread = None;
            }
            Message::NavigateToSettings => {
                self.previous_view = self.active_view;
                self.active_view = ActiveView::Settings;
                self.drawer_open = false;
            }
            Message::NavigateBack => {
                self.active_view = self.previous_view;
                self.drawer_open = true;
            }
            Message::MoveTo {
                thread_id,
                destination,
            } => {
                tracing::info!("move thread {thread_id} to {destination:?}");
                self.overflow_menu_thread = None;
                self.context_menu_thread = None;
            }
            Message::MarkReadState { thread_id, read } => {
                tracing::info!("mark thread {thread_id} read={read}");
                self.overflow_menu_thread = None;
                self.context_menu_thread = None;
            }
            Message::MuteThread(thread_id) => {
                tracing::info!("mute thread {thread_id}");
                self.overflow_menu_thread = None;
                self.context_menu_thread = None;
            }
            Message::Reply(thread_id) => {
                tracing::info!("reply to thread {thread_id}");
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
            Message::AddToBundle {
                thread_id,
                category,
            } => {
                tracing::info!("add thread {thread_id} to bundle {category}");
                self.overflow_menu_thread = None;
                self.context_menu_thread = None;
            }
            Message::CreateRuleFromSender(sender) => {
                tracing::info!("create rule from sender: {sender} (coming soon)");
                self.overflow_menu_thread = None;
                self.context_menu_thread = None;
            }
            Message::BlockSender {
                thread_id,
                sender_address,
            } => {
                tracing::info!("block sender {sender_address} (thread {thread_id})");
                self.overflow_menu_thread = None;
                self.context_menu_thread = None;
            }
            Message::ReportSpam(thread_id) => {
                tracing::info!("report spam: thread {thread_id}");
                self.overflow_menu_thread = None;
                self.context_menu_thread = None;
            }
        }
        Task::none()
    }

    /// Iced view function -- render the entire UI.
    pub fn view(&self) -> Element<'_, Message> {
        use crate::nav::view_drawer;
        use crate::toolbar::view_toolbar;

        let toolbar = view_toolbar(self);

        let drawer = if self.drawer_open {
            Some(view_drawer(self))
        } else {
            None
        };

        // Render inbox feed or placeholder depending on active view.
        let content_area: Element<Message> = if self.active_view == ActiveView::Inbox {
            inbox_view(
                &self.feed_sections,
                self.theme.colors.text_primary,
                self.theme.colors.text_secondary,
                self.theme.colors.surface,
                self.theme.colors.divider,
            )
            .map(Message::InboxView)
        } else {
            container(
                text(format!(
                    "{} -- content area placeholder",
                    self.active_view.title()
                ))
                .size(16.0),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(crate::theme::DEFAULT_PADDING)
            .into()
        };

        let body: Element<Message> = match drawer {
            Some(drawer_el) => row![drawer_el, content_area]
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            None => content_area,
        };

        let mut main_column = column![toolbar, body]
            .width(Length::Fill)
            .height(Length::Fill);

        // Undo snackbar at the bottom.
        if let Some(desc) = self.undo_state.description() {
            main_column = main_column.push(crate::widgets::undo_snackbar::undo_snackbar(
                &desc,
                Message::Undo,
                self.theme.colors.surface,
                self.theme.colors.text_primary,
                self.theme.colors.toolbar_inbox,
            ));
        }

        main_column.into()
    }

    /// Window title.
    pub fn title(&self) -> String {
        format!("Inboxly -- {}", self.active_view.title())
    }

    /// Iced theme -- returns the current theme for widget styling.
    pub fn theme(&self) -> Theme {
        self.theme.iced_theme().clone()
    }

    /// Reload the feed from the store (synchronous, fast).
    fn reload_feed(&mut self) {
        if let Some(ref store) = self.store {
            match feed::build_feed(store) {
                Ok(sections) => self.feed_sections = sections,
                Err(e) => {
                    tracing::warn!("failed to load inbox feed: {e}");
                    self.feed_sections = Vec::new();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nav::NavSection;

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
        assert_eq!(app.active_view, ActiveView::Inbox);
        assert_eq!(app.active_nav, NavTarget::Section(NavSection::Drafts));
    }

    #[test]
    fn navigate_to_bundle_category_does_not_change_active_view() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::Navigate(NavTarget::BundleCategory(
            "Social".into(),
        )));
        assert_eq!(app.active_view, ActiveView::Inbox);
        assert_eq!(app.active_nav, NavTarget::BundleCategory("Social".into()));
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

    // -- M16 theme tests --

    #[test]
    fn default_theme_is_light() {
        let app = Inboxly::default();
        assert!(!app.theme.colors.is_dark);
    }

    #[test]
    fn theme_toggle_changes_to_dark() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::ThemeToggled);
        assert!(app.theme.colors.is_dark);
    }

    #[test]
    fn theme_toggle_back_to_light() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::ThemeToggled);
        let _ = app.update(Message::ThemeToggled);
        assert!(!app.theme.colors.is_dark);
    }

    #[test]
    fn theme_changed_message_updates_theme() {
        let mut app = Inboxly::default();
        let dark = InboxlyTheme::dark();
        let _ = app.update(Message::ThemeChanged(dark));
        assert!(app.theme.colors.is_dark);
    }

    // -- M17 feed tests --

    #[test]
    fn default_feed_is_empty() {
        let app = Inboxly::default();
        assert!(app.feed_sections.is_empty());
    }

    #[test]
    fn reload_feed_with_store() {
        let store = Store::open_in_memory().expect("in-memory store");
        let (app, _) = Inboxly::with_store(store);
        assert!(app.feed_sections.is_empty());
    }

    #[test]
    fn reload_feed_message_does_not_panic() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::ReloadFeed);
        assert!(app.feed_sections.is_empty());
    }
}
