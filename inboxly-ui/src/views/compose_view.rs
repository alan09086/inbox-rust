//! Compose view -- new email, reply, reply-all, forward.
//!
//! Layout (from spec):
//! ```text
//! +----------------------------------------------------------+
//! | To: [recipients]                                          |
//! | Cc: [recipients]                                          |
//! | Subject: [subject line]                           18sp    |
//! +----------------------------------------------------------+
//! |                                                           |
//! | [body text area]                               16sp       |
//! |                                                           |
//! | max-width: 920dp                                          |
//! +----------------------------------------------------------+
//! | [Attach] [Discard]                          [Send]        |
//! +----------------------------------------------------------+
//! ```

use iced::widget::{Space, button, column, container, row, text, text_input};
use iced::{Alignment, Background, Border, Color, Element, Length};

use crate::theme::dimensions::{COMPOSE_MAX_WIDTH, DEFAULT_PADDING};
use crate::theme::typography::COMPOSE_SUBJECT_SIZE;

/// Compose state -- tracks the draft being edited.
#[derive(Debug, Clone, Default)]
pub struct ComposeState {
    /// "To" recipients (comma-separated).
    pub to: String,
    /// "Cc" recipients (comma-separated).
    pub cc: String,
    /// Email subject.
    pub subject: String,
    /// Email body (plaintext, Markdown rendering deferred to M25).
    pub body: String,
    /// Whether the Cc field is visible.
    pub show_cc: bool,
    /// Compose mode: New, Reply, ReplyAll, or Forward.
    pub mode: ComposeMode,
    /// Original thread ID (for replies/forwards).
    pub reply_to_thread: Option<String>,
}

/// Compose mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ComposeMode {
    /// Composing a new email.
    #[default]
    New,
    /// Replying to a single sender.
    Reply,
    /// Replying to all recipients.
    ReplyAll,
    /// Forwarding an email.
    Forward,
}

/// Messages from the compose view.
#[derive(Debug, Clone)]
pub enum ComposeMessage {
    /// "To" field changed.
    ToChanged(String),
    /// "Cc" field changed.
    CcChanged(String),
    /// Subject field changed.
    SubjectChanged(String),
    /// Body field changed.
    BodyChanged(String),
    /// User pressed Send.
    Send,
    /// User pressed Discard.
    Discard,
    /// Toggle Cc field visibility.
    ToggleCc,
}

/// Build the compose view element.
pub fn compose_view<'a>(
    state: &ComposeState,
    primary_text: Color,
    secondary_text: Color,
    surface: Color,
    accent: Color,
) -> Element<'a, ComposeMessage> {
    let mode_label = match state.mode {
        ComposeMode::New => "New Message",
        ComposeMode::Reply => "Reply",
        ComposeMode::ReplyAll => "Reply All",
        ComposeMode::Forward => "Forward",
    };

    let header = text(mode_label)
        .size(COMPOSE_SUBJECT_SIZE)
        .color(primary_text)
        .font(iced::Font {
            weight: iced::font::Weight::Bold,
            ..Default::default()
        });

    let to_field = text_input("To", &state.to)
        .on_input(ComposeMessage::ToChanged)
        .padding([8.0, 12.0]);

    let mut fields = column![header, to_field].spacing(8.0);

    if state.show_cc {
        let cc_field = text_input("Cc", &state.cc)
            .on_input(ComposeMessage::CcChanged)
            .padding([8.0, 12.0]);
        fields = fields.push(cc_field);
    }

    let subject_field = text_input("Subject", &state.subject)
        .on_input(ComposeMessage::SubjectChanged)
        .padding([8.0, 12.0]);

    let body_field = text_input("Compose email", &state.body)
        .on_input(ComposeMessage::BodyChanged)
        .padding([12.0, 12.0]);

    let send_btn = button(text("Send").size(14.0).color(Color::WHITE))
        .on_press(ComposeMessage::Send)
        .padding([8.0, 24.0])
        .style(move |_theme, _status| button::Style {
            background: Some(Background::Color(accent)),
            border: Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

    let discard_btn = button(text("Discard").size(14.0).color(secondary_text))
        .on_press(ComposeMessage::Discard)
        .padding([8.0, 16.0])
        .style(move |_theme, _status| button::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            border: Border::default(),
            ..Default::default()
        });

    let toolbar = row![discard_btn, Space::new().width(Length::Fill), send_btn]
        .align_y(Alignment::Center)
        .padding([8.0, 0.0]);

    let compose = column![fields, subject_field, body_field, toolbar]
        .spacing(12.0)
        .padding(DEFAULT_PADDING)
        .max_width(COMPOSE_MAX_WIDTH);

    container(compose)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Center)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(surface)),
            ..Default::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_compose_mode_is_new() {
        let state = ComposeState::default();
        assert_eq!(state.mode, ComposeMode::New);
    }

    #[test]
    fn compose_state_fields_empty_by_default() {
        let state = ComposeState::default();
        assert!(state.to.is_empty());
        assert!(state.cc.is_empty());
        assert!(state.subject.is_empty());
        assert!(state.body.is_empty());
        assert!(!state.show_cc);
    }
}
