//! Settings view -- sidebar navigation + tab content.
//!
//! Layout: 240px sidebar with tab buttons | scrollable content area (max 640px).

use iced::widget::{Space, button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Background, Border, Color, Element, Length};

use inboxly_core::config::{AuthMethod, ThemePreference};

use crate::app::{Inboxly, Message};
use crate::theme::colors::ThemeColors;
use crate::theme::{SettingsReader, SettingsWriter};
use crate::views::{settings_bundles, settings_notifications, settings_shortcuts};

// -- Constants --

/// Sidebar width in logical pixels.
const SIDEBAR_WIDTH: f32 = 240.0;
/// Content area max width.
const CONTENT_MAX_WIDTH: f32 = 640.0;
/// Section header font size.
const SECTION_HEADER_SIZE: f32 = 16.0;
/// Google blue accent (#4285f4).
const ACCENT_BLUE: Color = crate::theme::colors::hex("#4285f4");
/// Light blue for active tab background.
const ACTIVE_TAB_BG: Color = Color {
    r: 0.91,
    g: 0.95,
    b: 1.0,
    a: 1.0,
};
/// Dark theme active tab background.
const ACTIVE_TAB_BG_DARK: Color = Color {
    r: 0.10,
    g: 0.23,
    b: 0.44,
    a: 1.0,
};
/// Red for destructive actions.
const DESTRUCTIVE_RED: Color = crate::theme::colors::hex("#ef5350");

// ============================================================================
// SettingsTab enum + StoreSettingsAdapter (data layer from Batch 1)
// ============================================================================

/// The six tabs in the settings sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SettingsTab {
    #[default]
    General,
    Accounts,
    Bundles,
    Notifications,
    KeyboardShortcuts,
    DataStorage,
}

impl SettingsTab {
    /// Display label for the sidebar.
    pub fn label(&self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Accounts => "Accounts",
            Self::Bundles => "Bundles",
            Self::Notifications => "Notifications",
            Self::KeyboardShortcuts => "Keyboard Shortcuts",
            Self::DataStorage => "Data & Storage",
        }
    }

    /// All tabs in display order.
    pub fn all() -> &'static [Self] {
        &[
            Self::General,
            Self::Accounts,
            Self::Bundles,
            Self::Notifications,
            Self::KeyboardShortcuts,
            Self::DataStorage,
        ]
    }
}

/// Adapter that wraps an `inboxly_store::Store` reference and implements
/// the `SettingsReader`/`SettingsWriter` traits from `inboxly-ui`.
///
/// This avoids a circular dependency between `inboxly-store` and `inboxly-ui`.
pub struct StoreSettingsAdapter<'a> {
    pub store: &'a inboxly_store::Store,
}

impl SettingsReader for StoreSettingsAdapter<'_> {
    fn get_setting(&self, key: &str) -> Result<Option<String>, Box<dyn std::error::Error>> {
        self.store
            .get_setting(key)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
    }
}

impl SettingsWriter for StoreSettingsAdapter<'_> {
    fn set_setting(&self, key: &str, value: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.store
            .set_setting(key, value)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
    }
}

// ============================================================================
// Main settings view
// ============================================================================

/// Render the full settings view (sidebar + content).
pub fn settings_view(app: &Inboxly) -> Element<'_, Message> {
    let colors = &app.theme.colors;

    // Build sidebar
    let sidebar = settings_sidebar(app.settings_tab, colors);

    // Build content area based on active tab
    let content: Element<'_, Message> = match app.settings_tab {
        SettingsTab::General => general_tab(app),
        SettingsTab::Accounts => accounts_tab(app),
        SettingsTab::Bundles => settings_bundles::bundles_settings_tab(app),
        SettingsTab::Notifications => settings_notifications::notifications_settings_tab(app),
        SettingsTab::KeyboardShortcuts => settings_shortcuts::shortcuts_settings_tab(app),
        SettingsTab::DataStorage => data_storage_tab(app),
    };

    // Wrap content in scrollable with max width
    let scrollable_content = scrollable(
        container(content)
            .max_width(CONTENT_MAX_WIDTH)
            .padding([24.0, 32.0]),
    )
    .width(Length::Fill)
    .height(Length::Fill);

    let content_area = container(scrollable_content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(colors.background)),
            ..Default::default()
        });

    row![sidebar, content_area]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

// ============================================================================
// Sidebar
// ============================================================================

/// Render the settings sidebar with tab buttons.
fn settings_sidebar(active_tab: SettingsTab, colors: &ThemeColors) -> Element<'_, Message> {
    let mut col = column![].spacing(2).padding([16.0, 0.0]);

    for &tab in SettingsTab::all() {
        let is_active = tab == active_tab;
        let label_color = if is_active {
            ACCENT_BLUE
        } else {
            colors.text_primary
        };
        let bg_color = if is_active {
            if colors.is_dark {
                ACTIVE_TAB_BG_DARK
            } else {
                ACTIVE_TAB_BG
            }
        } else {
            Color::TRANSPARENT
        };
        let left_border_color = if is_active {
            ACCENT_BLUE
        } else {
            Color::TRANSPARENT
        };

        let label = text(tab.label()).size(14.0).color(label_color);

        let tab_button = button(
            row![
                container(Space::new().width(3.0)).style(move |_theme| container::Style {
                    background: Some(Background::Color(left_border_color)),
                    ..Default::default()
                }),
                container(label).padding([0.0, 12.0]),
            ]
            .align_y(Alignment::Center),
        )
        .on_press(Message::SettingsTabChanged(tab))
        .width(Length::Fill)
        .padding([10, 0])
        .style(move |_theme, _status| button::Style {
            background: Some(Background::Color(bg_color)),
            text_color: label_color,
            border: Border::default(),
            ..Default::default()
        });

        col = col.push(tab_button);
    }

    let surface = colors.surface;
    let divider = colors.divider;

    container(col)
        .width(SIDEBAR_WIDTH)
        .height(Length::Fill)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(surface)),
            border: Border {
                color: divider,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
}

// ============================================================================
// General tab
// ============================================================================

/// Render the General settings tab.
fn general_tab(app: &Inboxly) -> Element<'_, Message> {
    let colors = &app.theme.colors;

    // -- Theme section --
    let theme_header = section_header("Theme", colors);
    let theme_chips = row![
        chip_button(
            "System",
            app.theme_preference == ThemePreference::System,
            Message::SetThemePreference(ThemePreference::System),
            colors,
        ),
        chip_button(
            "Light",
            app.theme_preference == ThemePreference::Light,
            Message::SetThemePreference(ThemePreference::Light),
            colors,
        ),
        chip_button(
            "Dark",
            app.theme_preference == ThemePreference::Dark,
            Message::SetThemePreference(ThemePreference::Dark),
            colors,
        ),
    ]
    .spacing(8);

    // -- Default View section --
    let view_header = section_header("Default View", colors);
    let view_chips = row![
        chip_button(
            "Inbox",
            app.default_view == "inbox",
            Message::SetDefaultView("inbox".to_owned()),
            colors,
        ),
        chip_button(
            "Snoozed",
            app.default_view == "snoozed",
            Message::SetDefaultView("snoozed".to_owned()),
            colors,
        ),
        chip_button(
            "Done",
            app.default_view == "done",
            Message::SetDefaultView("done".to_owned()),
            colors,
        ),
    ]
    .spacing(8);

    // -- Snooze Presets section --
    let snooze_header = section_header("Snooze Presets", colors);
    let morning_str = app.config.snooze.morning_hour.to_string();
    let afternoon_str = app.config.snooze.afternoon_hour.to_string();
    let evening_str = app.config.snooze.evening_hour.to_string();
    let weekend_str = app.config.snooze.weekend_day.to_string();
    let snooze_fields = column![
        labeled_input(
            "Morning hour (0-23)",
            &morning_str,
            Message::SetSnoozeMorningHour,
            colors,
        ),
        labeled_input(
            "Afternoon hour (0-23)",
            &afternoon_str,
            Message::SetSnoozeAfternoonHour,
            colors,
        ),
        labeled_input(
            "Evening hour (0-23)",
            &evening_str,
            Message::SetSnoozeEveningHour,
            colors,
        ),
        labeled_input(
            "Weekend day (0=Mon..6=Sun)",
            &weekend_str,
            Message::SetSnoozeWeekendDay,
            colors,
        ),
    ]
    .spacing(8);

    // -- Undo Timeout section --
    let undo_header = section_header("Undo Timeout", colors);
    let undo_chips = row![
        chip_button(
            "3s",
            app.undo_timeout_secs == 3,
            Message::SetUndoTimeout(3),
            colors,
        ),
        chip_button(
            "5s",
            app.undo_timeout_secs == 5,
            Message::SetUndoTimeout(5),
            colors,
        ),
        chip_button(
            "7s",
            app.undo_timeout_secs == 7,
            Message::SetUndoTimeout(7),
            colors,
        ),
        chip_button(
            "10s",
            app.undo_timeout_secs == 10,
            Message::SetUndoTimeout(10),
            colors,
        ),
        chip_button(
            "15s",
            app.undo_timeout_secs == 15,
            Message::SetUndoTimeout(15),
            colors,
        ),
    ]
    .spacing(8);

    column![
        theme_header,
        theme_chips,
        Space::new().height(16.0),
        view_header,
        view_chips,
        Space::new().height(16.0),
        snooze_header,
        snooze_fields,
        Space::new().height(16.0),
        undo_header,
        undo_chips,
    ]
    .spacing(8)
    .width(Length::Fill)
    .into()
}

// ============================================================================
// Accounts tab
// ============================================================================

/// Render the Accounts settings tab.
fn accounts_tab(app: &Inboxly) -> Element<'_, Message> {
    let colors = &app.theme.colors;

    // Header row with "+ Add Account" button
    let header = row![
        text("Accounts")
            .size(SECTION_HEADER_SIZE)
            .color(colors.text_primary)
            .font(iced::Font::with_name("sans-serif")),
        Space::new().width(Length::Fill),
        button(text("+ Add Account").size(14.0).color(ACCENT_BLUE))
            .on_press(Message::AddAccountStart)
            .padding([6, 12])
            .style(move |_theme, _status| button::Style {
                background: Some(Background::Color(Color::TRANSPARENT)),
                text_color: ACCENT_BLUE,
                border: Border {
                    color: ACCENT_BLUE,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            }),
    ]
    .align_y(Alignment::Center);

    let mut content = column![header, Space::new().height(12.0)].spacing(8);

    // Add account form
    if app.adding_account {
        content = content.push(account_form(app, None));
    }

    // Existing account cards
    for (i, account) in app.config.accounts.iter().enumerate() {
        if app.editing_account_index == Some(i) {
            // Show edit form instead of card
            content = content.push(account_form(app, Some(i)));
        } else if app.removing_account_index == Some(i) {
            // Show account card with removal confirmation
            content = content.push(account_card(account, i, colors, app.active_account_index));
            content = content.push(removal_confirmation(i, colors));
        } else {
            content = content.push(account_card(account, i, colors, app.active_account_index));
        }
    }

    if app.config.accounts.is_empty() && !app.adding_account {
        content = content.push(
            text("No accounts configured. Click \"+ Add Account\" to get started.")
                .size(14.0)
                .color(colors.text_secondary),
        );
    }

    content.width(Length::Fill).into()
}

/// Render an account card showing email, provider, and action buttons.
fn account_card<'a>(
    account: &'a inboxly_core::config::AccountConfig,
    index: usize,
    colors: &'a ThemeColors,
    active_account_index: usize,
) -> Element<'a, Message> {
    let is_active = index == active_account_index;

    // Avatar circle with first letter
    let first_letter = account
        .email
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();
    let avatar = container(
        text(first_letter)
            .size(16.0)
            .color(Color::WHITE)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center),
    )
    .width(36.0)
    .height(36.0)
    .align_x(iced::alignment::Horizontal::Center)
    .align_y(iced::alignment::Vertical::Center)
    .style(move |_theme| container::Style {
        background: Some(Background::Color(ACCENT_BLUE)),
        border: Border {
            radius: 18.0.into(),
            ..Default::default()
        },
        ..Default::default()
    });

    // Account info
    let display = if account.display_name.is_empty() {
        account.email.clone()
    } else {
        format!("{} <{}>", account.display_name, account.email)
    };
    let provider_label = format!(
        "{} | {}",
        account.provider,
        match &account.auth_method {
            inboxly_core::config::AuthMethod::Password => "Password",
            inboxly_core::config::AuthMethod::OAuth2 => "OAuth2",
            inboxly_core::config::AuthMethod::AppPassword => "App Password",
        }
    );

    let info = column![
        text(display).size(14.0).color(colors.text_primary),
        text(provider_label).size(12.0).color(colors.text_secondary),
    ]
    .spacing(2);

    // Action buttons
    let edit_btn = button(text("Edit").size(13.0).color(ACCENT_BLUE))
        .on_press(Message::EditAccountStart(index))
        .padding([4, 8])
        .style(move |_theme, _status| button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            text_color: ACCENT_BLUE,
            border: Border::default(),
            ..Default::default()
        });

    let surface = colors.surface;
    let divider = colors.divider;

    // Remove button -- disabled for active account
    let remove_btn: Element<'_, Message> = if is_active {
        text("Remove")
            .size(13.0)
            .color(colors.text_secondary)
            .into()
    } else {
        button(text("Remove").size(13.0).color(DESTRUCTIVE_RED))
            .on_press(Message::RemoveAccountConfirm(index))
            .padding([4, 8])
            .style(move |_theme, _status| button::Style {
                background: Some(Background::Color(Color::TRANSPARENT)),
                text_color: DESTRUCTIVE_RED,
                border: Border::default(),
                ..Default::default()
            })
            .into()
    };

    let actions = row![edit_btn, remove_btn]
        .spacing(8)
        .align_y(Alignment::Center);

    let card_row = row![
        avatar,
        Space::new().width(12.0),
        info,
        Space::new().width(Length::Fill),
        actions,
    ]
    .align_y(Alignment::Center)
    .padding(12);

    container(card_row)
        .width(Length::Fill)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(surface)),
            border: Border {
                color: divider,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into()
}

/// Render the removal confirmation bar.
fn removal_confirmation<'a>(index: usize, colors: &'a ThemeColors) -> Element<'a, Message> {
    let surface = colors.surface;

    let bar = row![
        text("Remove this account?")
            .size(14.0)
            .color(DESTRUCTIVE_RED),
        Space::new().width(Length::Fill),
        button(text("Cancel").size(13.0).color(colors.text_primary))
            .on_press(Message::RemoveAccountCancel)
            .padding([4, 8])
            .style(move |_theme, _status| button::Style {
                background: Some(Background::Color(Color::TRANSPARENT)),
                text_color: Color::WHITE,
                border: Border::default(),
                ..Default::default()
            }),
        button(text("Remove").size(13.0).color(Color::WHITE))
            .on_press(Message::RemoveAccountExecute(index))
            .padding([4, 8])
            .style(move |_theme, _status| button::Style {
                background: Some(Background::Color(DESTRUCTIVE_RED)),
                text_color: Color::WHITE,
                border: Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .padding(8);

    container(bar)
        .width(Length::Fill)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(surface)),
            border: Border {
                color: DESTRUCTIVE_RED,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        })
        .into()
}

/// Render the account add/edit form.
fn account_form(app: &Inboxly, _edit_index: Option<usize>) -> Element<'_, Message> {
    let colors = &app.theme.colors;
    let form = &app.account_form;
    let surface = colors.surface;
    let divider = colors.divider;

    let title = if app.adding_account {
        "Add Account"
    } else {
        "Edit Account"
    };

    let auth_str = match form.auth_method {
        AuthMethod::Password => "password",
        AuthMethod::OAuth2 => "oauth2",
        AuthMethod::AppPassword => "app_password",
    };
    let imap_port_str = form.imap_port.to_string();
    let smtp_port_str = form.smtp_port.to_string();

    let fields = column![
        text(title)
            .size(SECTION_HEADER_SIZE)
            .color(colors.text_primary),
        form_field("Email", &form.email, Message::AccountFormEmailChanged),
        form_field(
            "Display Name",
            &form.display_name,
            Message::AccountFormDisplayNameChanged,
        ),
        form_field(
            "Provider",
            &form.provider,
            Message::AccountFormProviderChanged,
        ),
        form_field(
            "Auth Method (password/oauth2/app_password)",
            auth_str,
            Message::AccountFormAuthMethodChanged,
        ),
        form_field(
            "IMAP Host",
            &form.imap_host,
            Message::AccountFormImapHostChanged,
        ),
        form_field(
            "IMAP Port",
            &imap_port_str,
            Message::AccountFormImapPortChanged,
        ),
        form_field(
            "SMTP Host",
            &form.smtp_host,
            Message::AccountFormSmtpHostChanged,
        ),
        form_field(
            "SMTP Port",
            &smtp_port_str,
            Message::AccountFormSmtpPortChanged,
        ),
    ]
    .spacing(6);

    let buttons = row![
        button(text("Cancel").size(14.0).color(colors.text_primary))
            .on_press(Message::AccountFormCancel)
            .padding([6, 16])
            .style(move |_theme, _status| button::Style {
                background: Some(Background::Color(Color::TRANSPARENT)),
                border: Border {
                    color: divider,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            }),
        button(text("Save").size(14.0).color(Color::WHITE))
            .on_press(Message::AccountFormSave)
            .padding([6, 16])
            .style(move |_theme, _status| button::Style {
                background: Some(Background::Color(ACCENT_BLUE)),
                text_color: Color::WHITE,
                border: Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
    ]
    .spacing(8);

    let form_content = column![fields, Space::new().height(8.0), buttons]
        .spacing(4)
        .padding(16);

    container(form_content)
        .width(Length::Fill)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(surface)),
            border: Border {
                color: divider,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into()
}

/// A single labeled text input field for the account form.
fn form_field<F>(label: &str, value: &str, on_input: F) -> Element<'static, Message>
where
    F: Fn(String) -> Message + 'static,
{
    let label = label.to_owned();
    let value = value.to_owned();
    column![
        text(label.clone()).size(12.0),
        text_input(&label, &value)
            .on_input(on_input)
            .padding([6, 8]),
    ]
    .spacing(2)
    .into()
}

// ============================================================================
// Data & Storage tab
// ============================================================================

/// Render the Data & Storage settings tab.
fn data_storage_tab(app: &Inboxly) -> Element<'_, Message> {
    let colors = &app.theme.colors;
    let surface = colors.surface;
    let divider = colors.divider;

    // -- Action buttons --
    let actions_header = section_header("Actions", colors);
    let action_buttons = row![
        outline_button("Clear Cache", Message::ClearCache, colors),
        outline_button("Rebuild Search Index", Message::RebuildSearchIndex, colors,),
        outline_button("Export Data", Message::ExportData, colors),
    ]
    .spacing(8);

    // -- Status message --
    let status: Element<'_, Message> = if let Some(ref msg) = app.data_action_status {
        let status_color = if msg.starts_with("Failed") || msg.starts_with("Error") {
            DESTRUCTIVE_RED
        } else {
            crate::theme::colors::hex("#0f9d58") // green
        };
        text(msg).size(14.0).color(status_color).into()
    } else {
        Space::new().into()
    };

    // -- Storage info --
    let storage_header = section_header("Storage", colors);
    let storage_rows = column![
        info_row("Database size", &app.db_size_display, colors),
        info_row("Search index size", &app.index_size_display, colors),
        info_row("Maildir size", &app.maildir_size_display, colors),
    ]
    .spacing(4);

    // -- Sync info --
    let sync_header = section_header("Sync", colors);
    let sync_row = info_row("Last full sync", &app.last_sync_display, colors);

    let storage_content = column![
        storage_header,
        container(storage_rows)
            .padding(12)
            .width(Length::Fill)
            .style(move |_theme| container::Style {
                background: Some(Background::Color(surface)),
                border: Border {
                    color: divider,
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..Default::default()
            }),
    ]
    .spacing(8);

    let sync_content = column![
        sync_header,
        container(sync_row)
            .padding(12)
            .width(Length::Fill)
            .style(move |_theme| container::Style {
                background: Some(Background::Color(surface)),
                border: Border {
                    color: divider,
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..Default::default()
            }),
    ]
    .spacing(8);

    column![
        actions_header,
        action_buttons,
        status,
        Space::new().height(16.0),
        storage_content,
        Space::new().height(16.0),
        sync_content,
    ]
    .spacing(8)
    .width(Length::Fill)
    .into()
}

// ============================================================================
// Helper widgets
// ============================================================================

/// Section header text (bold, 16px).
fn section_header<'a>(label: &'a str, colors: &'a ThemeColors) -> Element<'a, Message> {
    text(label)
        .size(SECTION_HEADER_SIZE)
        .color(colors.text_primary)
        .font(iced::Font::with_name("sans-serif"))
        .into()
}

/// A chip button with conditional active styling.
fn chip_button<'a>(
    label: &'a str,
    is_active: bool,
    on_press: Message,
    colors: &'a ThemeColors,
) -> Element<'a, Message> {
    let (bg, text_color, border_color) = if is_active {
        (ACCENT_BLUE, Color::WHITE, ACCENT_BLUE)
    } else {
        (Color::TRANSPARENT, colors.text_primary, colors.divider)
    };

    button(text(label).size(13.0).color(text_color))
        .on_press(on_press)
        .padding([6, 14])
        .style(move |_theme, _status| button::Style {
            background: Some(Background::Color(bg)),
            text_color,
            border: Border {
                color: border_color,
                width: 1.0,
                radius: 16.0.into(),
            },
            ..Default::default()
        })
        .into()
}

/// An outline-style button for actions.
fn outline_button<'a>(
    label: &'a str,
    on_press: Message,
    colors: &'a ThemeColors,
) -> Element<'a, Message> {
    let text_color = colors.text_primary;
    let border_color = colors.divider;

    button(text(label).size(13.0).color(text_color))
        .on_press(on_press)
        .padding([8, 16])
        .style(move |_theme, _status| button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            text_color,
            border: Border {
                color: border_color,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        })
        .into()
}

/// A labeled text_input for snooze preset fields.
fn labeled_input<F>(
    label: &str,
    value: &str,
    on_input: F,
    _colors: &ThemeColors,
) -> Element<'static, Message>
where
    F: Fn(String) -> Message + 'static,
{
    let label = label.to_owned();
    let value = value.to_owned();
    row![
        text(label).size(13.0).width(Length::FillPortion(3)),
        text_input("", &value)
            .on_input(on_input)
            .width(Length::FillPortion(1))
            .padding([4, 8]),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

/// A read-only info row with label and value.
fn info_row<'a>(label: &'a str, value: &'a str, colors: &'a ThemeColors) -> Element<'a, Message> {
    row![
        text(label).size(14.0).color(colors.text_secondary),
        Space::new().width(Length::Fill),
        text(value).size(14.0).color(colors.text_primary),
    ]
    .align_y(Alignment::Center)
    .padding([4, 0])
    .into()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_tab_default_is_general() {
        assert_eq!(SettingsTab::default(), SettingsTab::General);
    }

    #[test]
    fn settings_tab_labels() {
        assert_eq!(SettingsTab::General.label(), "General");
        assert_eq!(SettingsTab::Accounts.label(), "Accounts");
        assert_eq!(SettingsTab::Bundles.label(), "Bundles");
        assert_eq!(SettingsTab::Notifications.label(), "Notifications");
        assert_eq!(SettingsTab::KeyboardShortcuts.label(), "Keyboard Shortcuts");
        assert_eq!(SettingsTab::DataStorage.label(), "Data & Storage");
    }

    #[test]
    fn settings_tab_all_returns_six_tabs() {
        let all = SettingsTab::all();
        assert_eq!(all.len(), 6);
        assert_eq!(all[0], SettingsTab::General);
        assert_eq!(all[5], SettingsTab::DataStorage);
    }

    #[test]
    fn settings_tab_all_order() {
        let all = SettingsTab::all();
        assert_eq!(all[0], SettingsTab::General);
        assert_eq!(all[1], SettingsTab::Accounts);
        assert_eq!(all[2], SettingsTab::Bundles);
        assert_eq!(all[3], SettingsTab::Notifications);
        assert_eq!(all[4], SettingsTab::KeyboardShortcuts);
        assert_eq!(all[5], SettingsTab::DataStorage);
    }

    #[test]
    fn store_settings_adapter_read_write() {
        let store = inboxly_store::Store::open_in_memory().expect("in-memory store");
        let adapter = StoreSettingsAdapter { store: &store };

        // Initially empty
        let val = adapter.get_setting("theme").expect("get_setting");
        assert!(val.is_none());

        // Write and read back
        adapter.set_setting("theme", "dark").expect("set_setting");
        let val = adapter.get_setting("theme").expect("get_setting");
        assert_eq!(val.as_deref(), Some("dark"));
    }

    #[test]
    fn store_settings_adapter_missing_key() {
        let store = inboxly_store::Store::open_in_memory().expect("in-memory store");
        let adapter = StoreSettingsAdapter { store: &store };

        let val = adapter.get_setting("nonexistent").expect("get_setting");
        assert!(val.is_none());
    }
}
