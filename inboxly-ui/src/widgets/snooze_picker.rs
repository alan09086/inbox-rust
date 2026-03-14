//! Snooze picker widget -- grid of preset snooze times.
//!
//! Layout: 2-column grid of snooze options, each showing an icon,
//! label, and computed time. Matches Google Inbox's snooze picker.
//!
//! ```text
//! +-------------------+-------------------+
//! | Later Today       | Tomorrow          |
//! | 6:00 PM           | 8:00 AM           |
//! +-------------------+-------------------+
//! | This Weekend      | Next Week         |
//! | Sat 8:00 AM       | Mon 8:00 AM       |
//! +-------------------+-------------------+
//! | Someday           | Pick date & time  |
//! | In 3 months       | Calendar picker   |
//! +-------------------+-------------------+
//! ```

use chrono::{DateTime, Datelike, Duration, Local, NaiveTime, Timelike, Utc, Weekday};

/// A snooze preset option.
#[derive(Debug, Clone)]
pub struct SnoozeOption {
    /// Display label (e.g., "Later Today").
    pub label: &'static str,
    /// Icon character (Unicode).
    pub icon: &'static str,
    /// Computed snooze-until time (UTC).
    pub until: DateTime<Utc>,
    /// Human-readable time description.
    pub time_display: String,
}

/// Compute the standard snooze presets relative to current time.
pub fn compute_presets() -> Vec<SnoozeOption> {
    let now = Local::now();
    let today = now.date_naive();

    vec![
        // Later Today: 6 PM today (or +3 hours if past 3 PM).
        {
            let target = if now.hour() >= 15 {
                now + Duration::hours(3)
            } else {
                today
                    .and_time(NaiveTime::from_hms_opt(18, 0, 0).expect("valid time"))
                    .and_local_timezone(Local)
                    .single()
                    .unwrap_or(now + Duration::hours(3))
            };
            SnoozeOption {
                label: "Later Today",
                icon: "\u{2600}", // sun
                until: target.with_timezone(&Utc),
                time_display: target.format("%-I:%M %p").to_string(),
            }
        },
        // Tomorrow: 8 AM tomorrow.
        {
            let tomorrow = today + Duration::days(1);
            let target = tomorrow
                .and_time(NaiveTime::from_hms_opt(8, 0, 0).expect("valid time"))
                .and_local_timezone(Local)
                .single()
                .unwrap_or(now + Duration::days(1));
            SnoozeOption {
                label: "Tomorrow",
                icon: "\u{1F305}", // sunrise
                until: target.with_timezone(&Utc),
                time_display: target.format("%-I:%M %p").to_string(),
            }
        },
        // This Weekend: Saturday 8 AM.
        {
            let days_to_sat = (Weekday::Sat.num_days_from_monday() as i64
                - today.weekday().num_days_from_monday() as i64
                + 7)
                % 7;
            let days_to_sat = if days_to_sat == 0 { 7 } else { days_to_sat };
            let sat = today + Duration::days(days_to_sat);
            let target = sat
                .and_time(NaiveTime::from_hms_opt(8, 0, 0).expect("valid time"))
                .and_local_timezone(Local)
                .single()
                .unwrap_or(now + Duration::days(days_to_sat));
            SnoozeOption {
                label: "This Weekend",
                icon: "\u{1F3D6}", // beach
                until: target.with_timezone(&Utc),
                time_display: target.format("%a %-I:%M %p").to_string(),
            }
        },
        // Next Week: Monday 8 AM.
        {
            let days_to_mon = (Weekday::Mon.num_days_from_monday() as i64
                - today.weekday().num_days_from_monday() as i64
                + 7)
                % 7;
            let days_to_mon = if days_to_mon == 0 { 7 } else { days_to_mon };
            let mon = today + Duration::days(days_to_mon);
            let target = mon
                .and_time(NaiveTime::from_hms_opt(8, 0, 0).expect("valid time"))
                .and_local_timezone(Local)
                .single()
                .unwrap_or(now + Duration::days(days_to_mon));
            SnoozeOption {
                label: "Next Week",
                icon: "\u{1F4C5}", // calendar
                until: target.with_timezone(&Utc),
                time_display: target.format("%a %-I:%M %p").to_string(),
            }
        },
        // Someday: 3 months from now, 8 AM.
        {
            let future = today + Duration::days(90);
            let target = future
                .and_time(NaiveTime::from_hms_opt(8, 0, 0).expect("valid time"))
                .and_local_timezone(Local)
                .single()
                .unwrap_or(now + Duration::days(90));
            SnoozeOption {
                label: "Someday",
                icon: "\u{1F4AD}", // thought bubble
                until: target.with_timezone(&Utc),
                time_display: "In 3 months".to_owned(),
            }
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_returns_five_options() {
        let presets = compute_presets();
        assert_eq!(presets.len(), 5);
    }

    #[test]
    fn presets_are_in_future() {
        let now = Utc::now();
        for preset in compute_presets() {
            assert!(
                preset.until > now,
                "{} snooze time should be in the future",
                preset.label
            );
        }
    }

    #[test]
    fn last_preset_is_furthest_in_future() {
        let presets = compute_presets();
        let last = presets.last().expect("at least one preset");
        let first = &presets[0];
        assert!(
            last.until > first.until,
            "last preset ({}) should be after first ({})",
            last.label,
            first.label
        );
    }

    #[test]
    fn later_today_has_label() {
        let presets = compute_presets();
        assert_eq!(presets[0].label, "Later Today");
    }

    #[test]
    fn tomorrow_has_label() {
        let presets = compute_presets();
        assert_eq!(presets[1].label, "Tomorrow");
    }
}
