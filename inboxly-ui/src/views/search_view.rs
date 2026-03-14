//! Search results view -- displays results from full-text search.

use iced::widget::{Column, column, container, scrollable, text};
use iced::{Color, Element, Length};

use crate::theme::dimensions::DEFAULT_PADDING;

/// A single search result for display.
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// Thread ID for navigation.
    pub thread_id: String,
    /// Sender name.
    pub sender: String,
    /// Subject line.
    pub subject: String,
    /// Snippet with highlighted matches.
    pub snippet: String,
    /// Formatted date.
    pub date_display: String,
}

/// Messages from the search view.
#[derive(Debug, Clone)]
pub enum SearchViewMessage {
    /// User clicked a search result.
    OpenThread(String),
}

/// Build the search results view.
pub fn search_view<'a>(
    query: &str,
    results: &[SearchResult],
    primary_text: Color,
    secondary_text: Color,
    surface: Color,
) -> Element<'a, SearchViewMessage> {
    if query.is_empty() {
        return container(
            text("Start typing to search")
                .size(16.0)
                .color(secondary_text),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(DEFAULT_PADDING)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center)
        .into();
    }

    if results.is_empty() {
        return container(
            text(format!("No results for \"{query}\""))
                .size(16.0)
                .color(secondary_text),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(DEFAULT_PADDING)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center)
        .into();
    }

    let mut results_column = Column::new().width(Length::Fill).spacing(0.0);

    let result_count = results.len();
    results_column = results_column.push(
        container(
            text(format!(
                "{result_count} result{}",
                if result_count == 1 { "" } else { "s" }
            ))
            .size(14.0)
            .color(secondary_text),
        )
        .padding(DEFAULT_PADDING),
    );

    for result in results {
        let sender = text(result.sender.clone())
            .size(16.0)
            .color(primary_text)
            .font(iced::Font {
                weight: iced::font::Weight::Medium,
                ..Default::default()
            });

        let subject = text(result.subject.clone()).size(14.0).color(primary_text);

        let snippet = text(result.snippet.clone())
            .size(14.0)
            .color(secondary_text);

        let date = text(result.date_display.clone())
            .size(12.0)
            .color(secondary_text);

        let result_row = iced::widget::button(
            column![
                iced::widget::row![sender, iced::widget::Space::new().width(Length::Fill), date]
                    .align_y(iced::Alignment::Center),
                subject,
                snippet,
            ]
            .spacing(2.0)
            .padding(DEFAULT_PADDING),
        )
        .on_press(SearchViewMessage::OpenThread(result.thread_id.clone()))
        .width(Length::Fill)
        .style(move |_theme, _status| iced::widget::button::Style {
            background: Some(iced::Background::Color(surface)),
            border: iced::Border::default(),
            ..Default::default()
        });

        results_column = results_column.push(result_row);
    }

    scrollable(results_column)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// Simple query parser supporting basic search operators.
///
/// Syntax:
/// - `from:alice` -- search sender field
/// - `to:bob` -- search recipient field
/// - `subject:meeting` -- search subject only
/// - `has:attachment` -- filter for attachments
/// - `is:unread` -- filter for unread
/// - `before:2026-01-01` / `after:2025-06-01` -- date filters
/// - Everything else is full-text body search
#[derive(Debug, Clone, Default)]
pub struct ParsedQuery {
    /// Free-text terms for body/subject search.
    pub terms: Vec<String>,
    /// From: filter.
    pub from: Option<String>,
    /// To: filter.
    pub to: Option<String>,
    /// Subject: filter.
    pub subject: Option<String>,
    /// has:attachment filter.
    pub has_attachment: bool,
    /// is:unread filter.
    pub is_unread: bool,
}

/// Parse a search query string into structured filters.
pub fn parse_query(query: &str) -> ParsedQuery {
    let mut parsed = ParsedQuery::default();

    for token in query.split_whitespace() {
        if let Some(value) = token.strip_prefix("from:") {
            parsed.from = Some(value.to_owned());
        } else if let Some(value) = token.strip_prefix("to:") {
            parsed.to = Some(value.to_owned());
        } else if let Some(value) = token.strip_prefix("subject:") {
            parsed.subject = Some(value.to_owned());
        } else if token == "has:attachment" {
            parsed.has_attachment = true;
        } else if token == "is:unread" {
            parsed.is_unread = true;
        } else {
            parsed.terms.push(token.to_owned());
        }
    }

    parsed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_query() {
        let parsed = parse_query("");
        assert!(parsed.terms.is_empty());
        assert!(parsed.from.is_none());
    }

    #[test]
    fn parse_simple_terms() {
        let parsed = parse_query("hello world");
        assert_eq!(parsed.terms, vec!["hello", "world"]);
    }

    #[test]
    fn parse_from_filter() {
        let parsed = parse_query("from:alice meeting notes");
        assert_eq!(parsed.from, Some("alice".to_owned()));
        assert_eq!(parsed.terms, vec!["meeting", "notes"]);
    }

    #[test]
    fn parse_to_filter() {
        let parsed = parse_query("to:bob");
        assert_eq!(parsed.to, Some("bob".to_owned()));
    }

    #[test]
    fn parse_subject_filter() {
        let parsed = parse_query("subject:budget Q1");
        assert_eq!(parsed.subject, Some("budget".to_owned()));
        assert_eq!(parsed.terms, vec!["Q1"]);
    }

    #[test]
    fn parse_has_attachment() {
        let parsed = parse_query("has:attachment invoice");
        assert!(parsed.has_attachment);
        assert_eq!(parsed.terms, vec!["invoice"]);
    }

    #[test]
    fn parse_is_unread() {
        let parsed = parse_query("is:unread");
        assert!(parsed.is_unread);
    }

    #[test]
    fn parse_combined_filters() {
        let parsed = parse_query("from:alice subject:meeting has:attachment important");
        assert_eq!(parsed.from, Some("alice".to_owned()));
        assert_eq!(parsed.subject, Some("meeting".to_owned()));
        assert!(parsed.has_attachment);
        assert_eq!(parsed.terms, vec!["important"]);
    }
}
