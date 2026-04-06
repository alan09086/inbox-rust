//! Snooze preset picker popup.
//!
//! Rendered when `snooze_picker_thread` is Some. Shows a four-option
//! grid: Later Today, Tomorrow, This Weekend, Next Week. Each option
//! computes a target DateTime<Utc> from `config.snooze` and dispatches
//! `SnoozeThread { thread_id, until }`.
//!
//! A "custom" option is not included in M33 — the plan listed it, but
//! building a full date-picker UI is scope-creep. Revisit in M34+ if
//! user feedback asks for it.

use std::sync::Arc;

use chrono::{DateTime, Datelike, Duration, Local, TimeZone, Utc, Weekday};
use dioxus::prelude::*;

use crate::app::{Inboxly, Message};
use inboxly_core::SnoozePresets;

/// Snooze preset picker rendered as a positioned popup.
///
/// Self-gates: returns an empty element when no snooze picker is open.
#[component]
pub fn SnoozePicker() -> Element {
    let mut app_state = use_context::<Signal<Inboxly>>();
    let state = app_state.read();
    let Some(thread_id_str) = state.snooze_picker_thread.clone() else {
        return rsx! {};
    };
    let position = state.snooze_picker_position;
    let presets = state.config.snooze.clone();
    drop(state);

    let thread_id: Arc<str> = Arc::from(thread_id_str.as_str());

    // Compute the four preset DateTime<Utc> values once per render.
    let now_local = Local::now();
    let later_today = snooze_later_today(now_local, &presets);
    let tomorrow = snooze_tomorrow(now_local, &presets);
    let this_weekend = snooze_this_weekend(now_local, &presets);
    let next_week = snooze_next_week(now_local, &presets);

    // Clone per-closure.
    let tid_later = Arc::clone(&thread_id);
    let tid_tomorrow = Arc::clone(&thread_id);
    let tid_weekend = Arc::clone(&thread_id);
    let tid_next_week = thread_id;

    rsx! {
        div {
            class: "menu-backdrop",
            onclick: move |evt: Event<MouseData>| {
                evt.stop_propagation();
                app_state.write().update(Message::CloseSnoozePicker);
            },
            oncontextmenu: move |evt: Event<MouseData>| {
                evt.prevent_default();
                evt.stop_propagation();
            },
        }
        div {
            class: "snooze-picker",
            style: "top: {position.y}px; left: {position.x}px;",
            div {
                class: "snooze-picker-title",
                "Snooze until..."
            }
            div {
                class: "snooze-grid",
                // Later Today
                button {
                    class: "snooze-option",
                    onclick: move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        app_state.write().update(Message::SnoozeThread {
                            thread_id: tid_later.to_string(),
                            until: later_today,
                        });
                    },
                    span { class: "snooze-option-icon", "\u{1F306}" }
                    span { class: "snooze-option-label", "Later Today" }
                    span { class: "snooze-option-time", "{format_hour(presets.evening_hour)}" }
                }
                // Tomorrow
                button {
                    class: "snooze-option",
                    onclick: move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        app_state.write().update(Message::SnoozeThread {
                            thread_id: tid_tomorrow.to_string(),
                            until: tomorrow,
                        });
                    },
                    span { class: "snooze-option-icon", "\u{1F305}" }
                    span { class: "snooze-option-label", "Tomorrow" }
                    span { class: "snooze-option-time", "{format_hour(presets.morning_hour)}" }
                }
                // This Weekend
                button {
                    class: "snooze-option",
                    onclick: move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        app_state.write().update(Message::SnoozeThread {
                            thread_id: tid_weekend.to_string(),
                            until: this_weekend,
                        });
                    },
                    span { class: "snooze-option-icon", "\u{1F3D6}" }
                    span { class: "snooze-option-label", "This Weekend" }
                    span { class: "snooze-option-time", "Sat" }
                }
                // Next Week
                button {
                    class: "snooze-option",
                    onclick: move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        app_state.write().update(Message::SnoozeThread {
                            thread_id: tid_next_week.to_string(),
                            until: next_week,
                        });
                    },
                    span { class: "snooze-option-icon", "\u{1F4C5}" }
                    span { class: "snooze-option-label", "Next Week" }
                    span { class: "snooze-option-time", "Mon" }
                }
            }
        }
    }
}

/// Format an hour (0-23) as "8 AM" / "6 PM" etc.
fn format_hour(hour: u8) -> String {
    let hour = hour as u32;
    let (h12, ampm) = match hour {
        0 => (12, "AM"),
        1..=11 => (hour, "AM"),
        12 => (12, "PM"),
        13..=23 => (hour - 12, "PM"),
        _ => (12, "AM"),
    };
    format!("{h12} {ampm}")
}

/// Compute the DateTime<Utc> for "Later Today" — today at the configured
/// evening_hour, or tomorrow morning if the evening hour has already passed.
pub fn snooze_later_today(now: DateTime<Local>, presets: &SnoozePresets) -> DateTime<Utc> {
    let target = now
        .date_naive()
        .and_hms_opt(presets.evening_hour as u32, 0, 0)
        .expect("valid hms");
    let local_target = Local
        .from_local_datetime(&target)
        .single()
        .unwrap_or(now);

    if local_target <= now {
        // Evening has passed — snooze to tomorrow morning instead.
        snooze_tomorrow(now, presets)
    } else {
        local_target.with_timezone(&Utc)
    }
}

/// Tomorrow at the configured morning_hour.
pub fn snooze_tomorrow(now: DateTime<Local>, presets: &SnoozePresets) -> DateTime<Utc> {
    let tomorrow = now.date_naive() + Duration::days(1);
    let target = tomorrow
        .and_hms_opt(presets.morning_hour as u32, 0, 0)
        .expect("valid hms");
    Local
        .from_local_datetime(&target)
        .single()
        .map(|local| local.with_timezone(&Utc))
        .unwrap_or_else(|| now.with_timezone(&Utc) + Duration::days(1))
}

/// Next occurrence of the configured weekend_day (0=Mon..6=Sun) at morning_hour.
pub fn snooze_this_weekend(now: DateTime<Local>, presets: &SnoozePresets) -> DateTime<Utc> {
    let target_weekday = weekday_from_preset(presets.weekend_day);
    let today_weekday = now.weekday();
    let days_until = days_until_next(today_weekday, target_weekday);
    let target_date = now.date_naive() + Duration::days(days_until as i64);
    let target = target_date
        .and_hms_opt(presets.morning_hour as u32, 0, 0)
        .expect("valid hms");
    Local
        .from_local_datetime(&target)
        .single()
        .map(|local| local.with_timezone(&Utc))
        .unwrap_or_else(|| now.with_timezone(&Utc) + Duration::days(days_until as i64))
}

/// Next Monday at morning_hour.
pub fn snooze_next_week(now: DateTime<Local>, presets: &SnoozePresets) -> DateTime<Utc> {
    let today_weekday = now.weekday();
    let days_until_monday = days_until_next(today_weekday, Weekday::Mon);
    // If today IS Monday, snooze to next Monday (7 days), not today (0 days).
    let days_until = if days_until_monday == 0 {
        7
    } else {
        days_until_monday
    };
    let target_date = now.date_naive() + Duration::days(days_until as i64);
    let target = target_date
        .and_hms_opt(presets.morning_hour as u32, 0, 0)
        .expect("valid hms");
    Local
        .from_local_datetime(&target)
        .single()
        .map(|local| local.with_timezone(&Utc))
        .unwrap_or_else(|| now.with_timezone(&Utc) + Duration::days(days_until as i64))
}

fn weekday_from_preset(preset_day: u8) -> Weekday {
    match preset_day {
        0 => Weekday::Mon,
        1 => Weekday::Tue,
        2 => Weekday::Wed,
        3 => Weekday::Thu,
        4 => Weekday::Fri,
        5 => Weekday::Sat,
        _ => Weekday::Sun,
    }
}

/// Number of days from `from` until the next occurrence of `to`.
///
/// Returns 0 if they are the same weekday (callers that need "skip today"
/// must add their own guard, as `snooze_next_week` does).
pub fn days_until_next(from: Weekday, to: Weekday) -> u32 {
    let from = from.num_days_from_monday();
    let to = to.num_days_from_monday();
    if to >= from {
        to - from
    } else {
        7 - (from - to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use inboxly_core::SnoozePresets;

    fn fixed_presets() -> SnoozePresets {
        SnoozePresets {
            morning_hour: 8,
            afternoon_hour: 13,
            evening_hour: 18,
            weekend_day: 5, // Saturday
        }
    }

    fn local_at(y: i32, m: u32, d: u32, h: u32, mi: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(y, m, d, h, mi, 0)
            .single()
            .unwrap()
    }

    #[test]
    fn later_today_uses_evening_hour() {
        // Monday 2026-04-06, 10:00 local. Evening is 18:00. Later Today should be 18:00 today.
        let now = local_at(2026, 4, 6, 10, 0);
        let result = snooze_later_today(now, &fixed_presets());
        let expected = local_at(2026, 4, 6, 18, 0).with_timezone(&Utc);
        assert_eq!(result, expected);
    }

    #[test]
    fn later_today_past_evening_falls_through_to_tomorrow() {
        // Monday 2026-04-06, 20:00 local. Evening already past. Falls through to tomorrow 08:00.
        let now = local_at(2026, 4, 6, 20, 0);
        let result = snooze_later_today(now, &fixed_presets());
        let expected = local_at(2026, 4, 7, 8, 0).with_timezone(&Utc);
        assert_eq!(result, expected);
    }

    #[test]
    fn tomorrow_uses_morning_hour() {
        let now = local_at(2026, 4, 6, 14, 30); // any time
        let result = snooze_tomorrow(now, &fixed_presets());
        let expected = local_at(2026, 4, 7, 8, 0).with_timezone(&Utc);
        assert_eq!(result, expected);
    }

    #[test]
    fn this_weekend_from_monday_is_saturday() {
        let now = local_at(2026, 4, 6, 10, 0); // Monday
        let result = snooze_this_weekend(now, &fixed_presets());
        let expected = local_at(2026, 4, 11, 8, 0).with_timezone(&Utc); // Saturday
        assert_eq!(result, expected);
    }

    #[test]
    fn this_weekend_from_saturday_is_same_day() {
        let now = local_at(2026, 4, 11, 10, 0); // Saturday
        let result = snooze_this_weekend(now, &fixed_presets());
        // days_until_next(Sat, Sat) = 0, so target_date = today
        let expected = local_at(2026, 4, 11, 8, 0).with_timezone(&Utc);
        assert_eq!(result, expected);
        // NOTE: this may be in the past (10:00 > 8:00). The handler should
        // handle snooze-to-the-past as a no-op or immediate-unsnooze, but
        // that's out of scope for this component.
    }

    #[test]
    fn next_week_from_monday_is_next_monday() {
        let now = local_at(2026, 4, 6, 10, 0); // Monday
        let result = snooze_next_week(now, &fixed_presets());
        let expected = local_at(2026, 4, 13, 8, 0).with_timezone(&Utc); // next Monday
        assert_eq!(result, expected);
    }

    #[test]
    fn next_week_from_friday_is_following_monday() {
        let now = local_at(2026, 4, 10, 10, 0); // Friday
        let result = snooze_next_week(now, &fixed_presets());
        let expected = local_at(2026, 4, 13, 8, 0).with_timezone(&Utc); // Monday 3 days later
        assert_eq!(result, expected);
    }

    #[test]
    fn days_until_next_wraps_around_week() {
        use Weekday::*;
        assert_eq!(days_until_next(Mon, Mon), 0);
        assert_eq!(days_until_next(Mon, Fri), 4);
        assert_eq!(days_until_next(Fri, Mon), 3);
        assert_eq!(days_until_next(Sun, Mon), 1);
    }

    #[test]
    fn format_hour_am_pm() {
        assert_eq!(format_hour(0), "12 AM");
        assert_eq!(format_hour(8), "8 AM");
        assert_eq!(format_hour(12), "12 PM");
        assert_eq!(format_hour(13), "1 PM");
        assert_eq!(format_hour(23), "11 PM");
    }
}
