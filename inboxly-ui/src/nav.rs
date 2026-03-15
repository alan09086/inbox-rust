//! Navigation drawer types and view rendering.

use iced::widget::{Space, button, column, container, row, scrollable, text};
use iced::{Alignment, Background, Border, Color, Element, Length};

use crate::app::{Inboxly, Message};
use crate::theme::{
    AVATAR_DIAMETER, ActiveView, DEFAULT_PADDING, DIVIDER_THICKNESS, NAV_DRAWER_WIDTH,
    NAV_ITEM_HEIGHT, NAV_ITEM_SIZE, avatar_colors, category_color, color_from_hex,
    dimensions::{ACCOUNT_ROW_HEIGHT, ACCOUNT_SWITCHER_AVATAR},
    divider_color, primary_text, secondary_text, selected_bg, surface_color,
};

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
    /// Human-readable label for this section.
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
    pub fn all() -> &'static [Self] {
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
        NavBundleCategory {
            name: "Social".into(),
        },
        NavBundleCategory {
            name: "Promos".into(),
        },
        NavBundleCategory {
            name: "Updates".into(),
        },
        NavBundleCategory {
            name: "Finance".into(),
        },
        NavBundleCategory {
            name: "Purchases".into(),
        },
        NavBundleCategory {
            name: "Travel".into(),
        },
        NavBundleCategory {
            name: "Forums".into(),
        },
        NavBundleCategory {
            name: "Low Priority".into(),
        },
    ]
}

/// Unified navigation target -- any clickable item in the nav drawer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavTarget {
    /// Primary views (Inbox, Snoozed, Done) -- changes toolbar colour.
    View(ActiveView),
    /// Secondary nav (Drafts, Sent, etc.) -- loads folder content.
    Section(NavSection),
    /// Bundle category filter -- shows emails in that bundle.
    BundleCategory(String),
}

// === View rendering ===

/// Render a single nav item row (48dp tall, full width, selectable).
fn nav_item(
    label: &str,
    target: NavTarget,
    is_active: bool,
    dot_color: Option<Color>,
) -> Element<'_, Message> {
    let bg = if is_active {
        selected_bg()
    } else {
        surface_color()
    };

    let label_color = if is_active {
        color_from_hex(0x42, 0x85, 0xf4) // blue for active
    } else {
        primary_text()
    };

    let mut content_row = row![].spacing(12).align_y(Alignment::Center);

    // Optional coloured dot for bundle categories
    if let Some(dot) = dot_color {
        let dot_widget =
            container(Space::new().width(8.0).height(8.0)).style(move |_theme| container::Style {
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
        text(label.to_string())
            .size(NAV_ITEM_SIZE)
            .color(label_color),
    );

    let btn = button(
        container(content_row)
            .padding([0.0, DEFAULT_PADDING])
            .height(NAV_ITEM_HEIGHT)
            .width(Length::Fill)
            .align_y(iced::alignment::Vertical::Center),
    )
    .on_press(Message::Navigate(target))
    .width(Length::Fill)
    .style(move |_theme, _status| button::Style {
        background: Some(Background::Color(bg)),
        text_color: primary_text(),
        border: Border::default(),
        ..Default::default()
    });

    btn.into()
}

/// Render a horizontal divider line.
fn divider() -> Element<'static, Message> {
    container(Space::new().width(Length::Fill).height(DIVIDER_THICKNESS))
        .width(Length::Fill)
        .style(|_theme| container::Style {
            background: Some(Background::Color(divider_color())),
            ..Default::default()
        })
        .into()
}

/// Build a circular avatar with a letter and background colour.
fn letter_avatar(letter: char, diameter: f32) -> Element<'static, Message> {
    let letter_str = letter.to_uppercase().to_string();
    let bg_color = avatar_colors::for_letter(letter);
    container(
        text(letter_str)
            .size(diameter * 0.4)
            .color(Color::WHITE)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center),
    )
    .width(diameter)
    .height(diameter)
    .align_x(iced::alignment::Horizontal::Center)
    .align_y(iced::alignment::Vertical::Center)
    .style(move |_theme| container::Style {
        background: Some(Background::Color(bg_color)),
        border: Border {
            radius: (diameter / 2.0).into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .into()
}

/// Render the account switcher header -- always visible at top of drawer.
///
/// Shows the active account avatar (44px), display name (bold, 15px),
/// email (grey, 13px), and a chevron indicating open/closed state.
/// Clicking anywhere toggles the account list.
fn account_switcher_header(app: &Inboxly) -> Element<'_, Message> {
    let display_name = app.active_display_name();
    let email = app.active_email();

    let first_char = display_name.chars().next().unwrap_or('?');
    let avatar = letter_avatar(first_char, ACCOUNT_SWITCHER_AVATAR);

    let name_text = text(display_name.to_string())
        .size(15.0)
        .color(primary_text());

    let email_text = text(email.to_string()).size(13.0).color(secondary_text());

    let chevron = if app.account_switcher_open {
        "\u{25B2}" // ▲
    } else {
        "\u{25BC}" // ▼
    };
    let chevron_text = text(chevron.to_string()).size(12.0).color(secondary_text());

    let name_row = row![name_text, chevron_text]
        .spacing(6)
        .align_y(Alignment::Center);

    let info_col = column![name_row, email_text].spacing(2);

    let content = row![avatar, info_col]
        .spacing(12)
        .align_y(Alignment::Center)
        .padding(DEFAULT_PADDING);

    button(container(content).width(Length::Fill))
        .on_press(Message::ToggleAccountSwitcher)
        .width(Length::Fill)
        .style(|_theme, _status| button::Style {
            background: Some(Background::Color(surface_color())),
            text_color: primary_text(),
            border: Border::default(),
            ..Default::default()
        })
        .into()
}

/// Render the expanded account list (shown when `account_switcher_open` is true).
///
/// Each account row has a 40px avatar, email text, and the active account
/// gets a blue highlight background with a checkmark. Other accounts are
/// clickable to switch. An "Add account" row at the bottom navigates to Settings.
fn account_list(app: &Inboxly) -> Element<'_, Message> {
    let active_bg = color_from_hex(0xe8, 0xf0, 0xfe); // #e8f0fe
    let blue = color_from_hex(0x42, 0x85, 0xf4); // #4285f4

    let mut list = column![].spacing(0);

    for (index, account) in app.accounts.iter().enumerate() {
        let is_active = index == app.active_account_index;
        let first_char = if account.display_name.is_empty() {
            account.email.chars().next().unwrap_or('?')
        } else {
            account.display_name.chars().next().unwrap_or('?')
        };

        let avatar = letter_avatar(first_char, AVATAR_DIAMETER);

        let email_text = text(account.email.clone())
            .size(NAV_ITEM_SIZE)
            .color(primary_text());

        let mut content_row = row![avatar, email_text]
            .spacing(12)
            .align_y(Alignment::Center);

        if is_active {
            let check = text("\u{2713}".to_string()) // ✓
                .size(NAV_ITEM_SIZE)
                .color(blue);
            content_row = content_row.push(Space::new().width(Length::Fill).height(0.0));
            content_row = content_row.push(check);
        }

        let row_bg = if is_active {
            active_bg
        } else {
            surface_color()
        };

        let row_container = container(content_row)
            .padding([0.0, DEFAULT_PADDING])
            .height(ACCOUNT_ROW_HEIGHT)
            .width(Length::Fill)
            .align_y(iced::alignment::Vertical::Center);

        let row_element: Element<Message> = if is_active {
            // Active account row -- not clickable (already selected)
            container(row_container)
                .width(Length::Fill)
                .style(move |_theme| container::Style {
                    background: Some(Background::Color(row_bg)),
                    ..Default::default()
                })
                .into()
        } else {
            // Other account rows -- clickable to switch
            button(row_container)
                .on_press(Message::SwitchAccount(index))
                .width(Length::Fill)
                .style(move |_theme, _status| button::Style {
                    background: Some(Background::Color(row_bg)),
                    text_color: primary_text(),
                    border: Border::default(),
                    ..Default::default()
                })
                .into()
        };

        list = list.push(row_element);
    }

    // "Add account" row
    let plus_text = text("+".to_string()).size(20.0).color(blue);
    let add_label = text("Add account".to_string())
        .size(NAV_ITEM_SIZE)
        .color(blue);
    let add_row = row![plus_text, add_label]
        .spacing(12)
        .align_y(Alignment::Center);

    let add_container = container(add_row)
        .padding([0.0, DEFAULT_PADDING])
        .height(ACCOUNT_ROW_HEIGHT)
        .width(Length::Fill)
        .align_y(iced::alignment::Vertical::Center);

    let add_button = button(add_container)
        .on_press(Message::NavigateToSettings)
        .width(Length::Fill)
        .style(|_theme, _status| button::Style {
            background: Some(Background::Color(surface_color())),
            text_color: primary_text(),
            border: Border::default(),
            ..Default::default()
        });

    list = list.push(add_button);

    list.into()
}

/// Render the full nav drawer (264dp wide).
pub fn view_drawer(app: &Inboxly) -> Element<'_, Message> {
    let mut drawer = column![].width(NAV_DRAWER_WIDTH);

    // Account switcher header (always visible)
    drawer = drawer.push(account_switcher_header(app));
    if app.account_switcher_open {
        drawer = drawer.push(account_list(app));
    }
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
        container(text("Bundles").size(12.0).color(secondary_text()))
            .padding([12.0, DEFAULT_PADDING])
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

    drawer = drawer.push(scrollable(bundle_col).height(Length::Fill));

    container(drawer)
        .width(NAV_DRAWER_WIDTH)
        .height(Length::Fill)
        .style(|_theme| container::Style {
            background: Some(Background::Color(surface_color())),
            ..Default::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use crate::app::{Inboxly, Message};
    use inboxly_core::config::{AccountConfig, AuthMethod};

    fn make_test_account(email: &str, display_name: &str) -> AccountConfig {
        AccountConfig {
            email: email.to_string(),
            display_name: display_name.to_string(),
            provider: "generic".to_string(),
            auth_method: AuthMethod::Password,
            imap_host: "imap.example.com".to_string(),
            imap_port: 993,
            smtp_host: "smtp.example.com".to_string(),
            smtp_port: 587,
        }
    }

    #[test]
    fn account_switcher_starts_collapsed() {
        let app = Inboxly::default();
        assert!(!app.account_switcher_open);
    }

    #[test]
    fn account_switcher_toggles() {
        let mut app = Inboxly::default();
        let _ = app.update(Message::ToggleAccountSwitcher);
        assert!(app.account_switcher_open);
        let _ = app.update(Message::ToggleAccountSwitcher);
        assert!(!app.account_switcher_open);
    }

    #[test]
    fn switch_to_same_account_still_closes_switcher() {
        let mut app = Inboxly::default();
        app.accounts = vec![make_test_account("test@example.com", "Test User")];
        app.account_switcher_open = true;
        let _ = app.update(Message::SwitchAccount(0));
        assert!(!app.account_switcher_open);
        assert_eq!(app.active_account_index, 0);
    }

    #[test]
    fn switch_account_updates_index_and_closes() {
        let mut app = Inboxly::default();
        app.accounts = vec![
            make_test_account("first@example.com", "First"),
            make_test_account("second@example.com", "Second"),
        ];
        app.account_switcher_open = true;
        let _ = app.update(Message::SwitchAccount(1));
        assert_eq!(app.active_account_index, 1);
        assert_eq!(app.active_email(), "second@example.com");
        assert!(!app.account_switcher_open);
    }

    #[test]
    fn navigate_closes_account_switcher() {
        let mut app = Inboxly::default();
        app.account_switcher_open = true;
        let _ = app.update(Message::Navigate(super::NavTarget::View(
            crate::theme::ActiveView::Inbox,
        )));
        assert!(!app.account_switcher_open);
    }

    #[test]
    fn with_accounts_sets_accounts() {
        let accounts = vec![make_test_account("test@example.com", "Test")];
        let app = Inboxly::with_accounts(accounts);
        assert_eq!(app.accounts.len(), 1);
        assert_eq!(app.active_email(), "test@example.com");
    }
}
