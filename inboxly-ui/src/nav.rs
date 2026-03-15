//! Navigation drawer types and view rendering.

use iced::widget::{Space, button, column, container, row, scrollable, text};
use iced::{Alignment, Background, Border, Color, Element, Length};

use crate::app::{Inboxly, Message};
use crate::theme::{
    AVATAR_DIAMETER, ActiveView, DEFAULT_PADDING, DIVIDER_THICKNESS, NAV_DRAWER_WIDTH,
    NAV_ITEM_HEIGHT, NAV_ITEM_SIZE, category_color, color_from_hex, divider_color, primary_text,
    secondary_text, selected_bg, surface_color,
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

/// Render the account switcher at the top of the nav drawer.
fn account_switcher<'a>(app: &'a Inboxly) -> Element<'a, Message> {
    let email = app.active_email();
    let account_count = app.accounts.len();

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
        .color(primary_text());

    let suffix = if account_count != 1 { "s" } else { "" };
    let count_text = text(format!("{account_count} account{suffix}"))
        .size(12.0)
        .color(secondary_text());

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
pub fn view_drawer(app: &Inboxly) -> Element<'_, Message> {
    let mut drawer = column![].width(NAV_DRAWER_WIDTH);

    // Account switcher
    drawer = drawer.push(account_switcher(app));
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
