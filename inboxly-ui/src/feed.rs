//! Inbox feed data model -- date grouping, feed items, and store queries.
//!
//! Transforms raw store data into view-model types for the inbox feed.
//! Date grouping categorises threads by temporal proximity (Pinned / Today /
//! Yesterday / This Week / This Month / Earlier).

use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, Local, Utc};

use inboxly_store::Store;

/// Date-based grouping for inbox feed sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DateGroup {
    /// Pinned items -- always at the top, regardless of date.
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

        if let Some(yesterday) = today.pred_opt()
            && thread_date == yesterday
        {
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

/// A single displayable row in the inbox feed.
///
/// This is a view-model -- lightweight data extracted from store queries,
/// not the full `Thread`/`EmailMeta` types.
#[derive(Debug, Clone)]
pub struct FeedItem {
    /// Thread ID for navigation and actions.
    pub thread_id: String,
    /// Primary sender's display name (or email if no name).
    pub sender_name: String,
    /// Sender's email address (for block/rule actions).
    pub sender_address: String,
    /// Avatar letter (first letter of name, uppercased).
    pub avatar_letter: char,
    /// Avatar colour index (0-25, maps to A-Z palette).
    pub avatar_color_index: u8,
    /// Thread subject line.
    pub subject: String,
    /// Snippet -- first ~200 chars of newest email body, plaintext.
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

/// A single entry in the feed -- either an email thread or a collapsed bundle.
#[derive(Debug, Clone)]
pub enum FeedEntry {
    /// An individual email thread (unbundled or expanded from a bundle).
    Thread(FeedItem),
    /// A collapsed bundle summary.
    Bundle(inboxly_store::BundleSummary),
}

impl FeedEntry {
    /// Timestamp for sorting (newest date).
    pub fn newest_date(&self) -> DateTime<Utc> {
        match self {
            Self::Thread(item) => item.timestamp,
            Self::Bundle(summary) => summary.newest_date,
        }
    }
}

/// A section of the feed -- a date group header + its entries.
#[derive(Debug, Clone)]
pub struct FeedSection {
    /// The date group this section represents.
    pub group: DateGroup,
    /// Entries in this section, ordered newest-first.
    pub items: Vec<FeedEntry>,
}

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

    if let Some(yesterday) = today.pred_opt()
        && thread_date == yesterday
    {
        return "Yesterday".to_owned();
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

/// Query the store and build the feed sections for the inbox view.
///
/// Returns sections in display order (Pinned first, then chronological).
/// Empty sections are omitted.
///
/// # Errors
///
/// Returns a store error if the database query fails.
pub fn build_feed(store: &Store) -> Result<Vec<FeedSection>, inboxly_store::StoreError> {
    let threads = store.query_inbox_threads()?;
    let bundles = store.query_bundle_summaries()?;

    let mut pinned_entries = Vec::new();
    let mut grouped: BTreeMap<DateGroup, Vec<FeedEntry>> = BTreeMap::new();

    // query_inbox_threads returns all non-done threads (including bundled ones).
    // Bundled threads appear inside bundle rows, so unbundled threads are
    // the only individual rows. The query already filters to bundle_id IS NULL
    // in the WHERE clause, so we don't need additional filtering here.

    for thread in threads {
        let item = FeedItem {
            thread_id: thread.id,
            sender_address: thread.sender_address.clone(),
            sender_name: if thread.sender_name.is_empty() {
                thread.sender_address
            } else {
                thread.sender_name
            },
            avatar_letter: thread.avatar_letter,
            avatar_color_index: thread.avatar_color_index,
            subject: thread.subject,
            snippet: thread.snippet,
            timestamp: thread.newest_date,
            timestamp_display: format_timestamp(thread.newest_date),
            is_unread: thread.unread_count > 0,
            has_attachments: thread.has_attachments,
            is_pinned: thread.pinned,
            email_count: thread.email_count,
        };

        if thread.pinned {
            pinned_entries.push(FeedEntry::Thread(item));
        } else {
            let group = DateGroup::from_date(thread.newest_date);
            grouped
                .entry(group)
                .or_default()
                .push(FeedEntry::Thread(item));
        }
    }

    // Add bundle summaries into their date groups.
    for bundle in bundles {
        let group = DateGroup::from_date(bundle.newest_date);
        grouped
            .entry(group)
            .or_default()
            .push(FeedEntry::Bundle(bundle));
    }

    // Sort entries within each group by newest_date descending.
    for entries in grouped.values_mut() {
        entries.sort_by_key(|e| std::cmp::Reverse(e.newest_date()));
    }

    let mut sections = Vec::new();

    // Pinned section first (if any).
    if !pinned_entries.is_empty() {
        sections.push(FeedSection {
            group: DateGroup::Pinned,
            items: pinned_entries,
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
        if let Some(items) = grouped.remove(&group)
            && !items.is_empty()
        {
            sections.push(FeedSection { group, items });
        }
    }

    Ok(sections)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};

    #[test]
    fn today_classification() {
        let now = Utc::now();
        assert_eq!(DateGroup::from_date(now), DateGroup::Today);
    }

    #[test]
    fn yesterday_classification() {
        let yesterday = Utc::now() - Duration::days(1);
        assert_eq!(DateGroup::from_date(yesterday), DateGroup::Yesterday);
    }

    #[test]
    fn this_week_classification() {
        let five_days_ago = Utc::now() - Duration::days(5);
        assert_eq!(DateGroup::from_date(five_days_ago), DateGroup::ThisWeek);
    }

    #[test]
    fn earlier_classification() {
        let old = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();
        assert_eq!(DateGroup::from_date(old), DateGroup::Earlier);
    }

    #[test]
    fn date_group_labels() {
        assert_eq!(DateGroup::Pinned.label(), "Pinned");
        assert_eq!(DateGroup::Today.label(), "Today");
        assert_eq!(DateGroup::Yesterday.label(), "Yesterday");
        assert_eq!(DateGroup::ThisWeek.label(), "This Week");
        assert_eq!(DateGroup::ThisMonth.label(), "This Month");
        assert_eq!(DateGroup::Earlier.label(), "Earlier");
    }

    #[test]
    fn date_group_ordering() {
        assert!(DateGroup::Pinned < DateGroup::Today);
        assert!(DateGroup::Today < DateGroup::Yesterday);
        assert!(DateGroup::Yesterday < DateGroup::ThisWeek);
        assert!(DateGroup::ThisWeek < DateGroup::ThisMonth);
        assert!(DateGroup::ThisMonth < DateGroup::Earlier);
    }

    #[test]
    fn format_timestamp_today_shows_time() {
        let now = Utc::now();
        let formatted = format_timestamp(now);
        assert!(
            formatted.contains("AM") || formatted.contains("PM"),
            "today's timestamp should be time-only, got: {formatted}"
        );
    }

    #[test]
    fn format_timestamp_yesterday_shows_yesterday() {
        let yesterday = Utc::now() - Duration::days(1);
        assert_eq!(format_timestamp(yesterday), "Yesterday");
    }

    #[test]
    fn format_timestamp_this_week_shows_weekday() {
        let five_days_ago = Utc::now() - Duration::days(5);
        let formatted = format_timestamp(five_days_ago);
        let weekdays = [
            "Monday",
            "Tuesday",
            "Wednesday",
            "Thursday",
            "Friday",
            "Saturday",
            "Sunday",
        ];
        assert!(
            weekdays.iter().any(|d| formatted == *d),
            "5 days ago should be a weekday name, got: {formatted}"
        );
    }

    #[test]
    fn format_timestamp_old_includes_year() {
        let old = Utc.with_ymd_and_hms(2020, 6, 15, 12, 0, 0).unwrap();
        assert_eq!(format_timestamp(old), "Jun 15, 2020");
    }

    #[test]
    fn build_feed_empty_store() {
        let store = Store::open_in_memory().expect("in-memory store");
        let sections = build_feed(&store).expect("build_feed");
        assert!(sections.is_empty());
    }
}
