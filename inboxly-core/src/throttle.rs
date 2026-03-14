//! Bundle throttle configuration and delivery window computation.
//!
//! Throttling controls when a bundle's emails surface in the inbox feed.
//! Emails are always synced and stored -- throttling is presentation-only.
//!
//! ```text
//!   Email arrives
//!       |
//!       v
//!   Bundler categorises -> assigned to bundle
//!       |
//!       v
//!   Bundle throttle == Immediate?
//!       |           |
//!      yes         no
//!       |           |
//!       v           v
//!   Show in feed   Suppress until delivery window opens
//! ```

use chrono::{DateTime, Datelike, NaiveTime, Utc, Weekday};
use serde::{Deserialize, Serialize};

/// How a bundle delivers its emails to the inbox feed.
///
/// Stored as JSON in the `bundles.throttle` column, tagged by `mode`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode")]
pub enum BundleThrottle {
    /// Emails appear as they arrive (default).
    Immediate,

    /// Bundle surfaces once per day at the configured time.
    Daily {
        /// Time of day to deliver (e.g., 17:00 for 5 PM). Local time.
        delivery_time: NaiveTime,
    },

    /// Bundle surfaces once per week at the configured day and time.
    Weekly {
        /// Day of week to deliver (e.g., Monday).
        delivery_day: WeekdayWrapper,
        /// Time of day to deliver (e.g., 08:00 for 8 AM). Local time.
        delivery_time: NaiveTime,
    },
}

impl Default for BundleThrottle {
    fn default() -> Self {
        Self::Immediate
    }
}

impl BundleThrottle {
    /// Returns `true` if this throttle allows emails to surface right now.
    ///
    /// For `Immediate`, always returns `true`.
    /// For `Daily`, returns `true` if `now` is at or past `delivery_time` today.
    /// For `Weekly`, returns `true` if `now` is at or past `delivery_time` on
    /// `delivery_day`, and remains true until the next occurrence of that day/time.
    ///
    /// `local_now` should be the current time in the user's local timezone.
    pub fn is_window_open(&self, local_now: &DateTime<chrono::Local>) -> bool {
        match self {
            Self::Immediate => true,
            Self::Daily { delivery_time } => {
                let now_time = local_now.time();
                // Window is open from delivery_time until end of day.
                // Emails delivered at 5 PM remain visible until next 5 PM cycle.
                now_time >= *delivery_time
            }
            Self::Weekly {
                delivery_day,
                delivery_time,
            } => {
                let now_weekday = local_now.weekday();
                let now_time = local_now.time();
                if now_weekday == delivery_day.0 {
                    now_time >= *delivery_time
                } else {
                    // Check if we're in the days after delivery_day but before
                    // the next one. days_since returns 0 for same day, so any
                    // positive value means we're past delivery_day this week.
                    let since = days_since(delivery_day.0, now_weekday);
                    since > 0 && since < 7
                }
            }
        }
    }

    /// Returns the next time this throttle's delivery window opens.
    ///
    /// For `Immediate`, returns `None` (always open).
    /// For `Daily` and `Weekly`, returns the next delivery time as UTC.
    pub fn next_window(&self, local_now: &DateTime<chrono::Local>) -> Option<DateTime<Utc>> {
        match self {
            Self::Immediate => None,
            Self::Daily { delivery_time } => {
                let today = local_now.date_naive();
                let candidate = today.and_time(*delivery_time);
                let next = if local_now.naive_local() >= candidate {
                    // Already past today's window, next is tomorrow
                    candidate + chrono::Duration::days(1)
                } else {
                    candidate
                };
                Some(next.and_utc())
            }
            Self::Weekly {
                delivery_day,
                delivery_time,
            } => {
                let today = local_now.date_naive();
                let today_weekday = today.weekday();
                let days_ahead = days_until(today_weekday, delivery_day.0);
                let candidate_date =
                    today + chrono::Duration::days(i64::from(days_ahead));
                let candidate = candidate_date.and_time(*delivery_time);
                let next = if days_ahead == 0 && local_now.naive_local() >= candidate {
                    // Same day but past the time, next week
                    candidate + chrono::Duration::days(7)
                } else {
                    candidate
                };
                Some(next.and_utc())
            }
        }
    }

    /// Returns `true` if this throttle suppresses emails (is not Immediate).
    pub fn is_throttled(&self) -> bool {
        !matches!(self, Self::Immediate)
    }
}

/// Wrapper around `chrono::Weekday` for serde support.
///
/// `chrono::Weekday` does not implement `Serialize`/`Deserialize`,
/// so we wrap it and provide lowercase string serialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeekdayWrapper(pub Weekday);

impl Serialize for WeekdayWrapper {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(weekday_to_str(self.0))
    }
}

impl<'de> Deserialize<'de> for WeekdayWrapper {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        weekday_from_str(&s)
            .map(WeekdayWrapper)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid weekday: {s}")))
    }
}

fn weekday_to_str(w: Weekday) -> &'static str {
    match w {
        Weekday::Mon => "monday",
        Weekday::Tue => "tuesday",
        Weekday::Wed => "wednesday",
        Weekday::Thu => "thursday",
        Weekday::Fri => "friday",
        Weekday::Sat => "saturday",
        Weekday::Sun => "sunday",
    }
}

fn weekday_from_str(s: &str) -> Option<Weekday> {
    match s.to_lowercase().as_str() {
        "monday" | "mon" => Some(Weekday::Mon),
        "tuesday" | "tue" => Some(Weekday::Tue),
        "wednesday" | "wed" => Some(Weekday::Wed),
        "thursday" | "thu" => Some(Weekday::Thu),
        "friday" | "fri" => Some(Weekday::Fri),
        "saturday" | "sat" => Some(Weekday::Sat),
        "sunday" | "sun" => Some(Weekday::Sun),
        _ => None,
    }
}

/// Number of days from `from` to `to` going forward (0 if same day).
fn days_since(from: Weekday, to: Weekday) -> u32 {
    let from_num = from.num_days_from_monday();
    let to_num = to.num_days_from_monday();
    (to_num + 7 - from_num) % 7
}

/// Number of days until `target` from `current` (0 if same day).
fn days_until(current: Weekday, target: Weekday) -> u32 {
    let current_num = current.num_days_from_monday();
    let target_num = target.num_days_from_monday();
    if current_num == target_num {
        0 // Same day -- caller checks time to decide if 0 or 7
    } else {
        (target_num + 7 - current_num) % 7
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, TimeZone};

    /// Helper: create a local datetime from components.
    fn local(year: i32, month: u32, day: u32, hour: u32, min: u32) -> DateTime<chrono::Local> {
        let naive = NaiveDate::from_ymd_opt(year, month, day)
            .expect("valid date")
            .and_hms_opt(hour, min, 0)
            .expect("valid time");
        chrono::Local
            .from_local_datetime(&naive)
            .single()
            .expect("unambiguous local time")
    }

    #[test]
    fn immediate_always_open() {
        let throttle = BundleThrottle::Immediate;
        let now = chrono::Local::now();
        assert!(throttle.is_window_open(&now));
        assert!(throttle.next_window(&now).is_none());
        assert!(!throttle.is_throttled());
    }

    #[test]
    fn daily_before_window() {
        let throttle = BundleThrottle::Daily {
            delivery_time: NaiveTime::from_hms_opt(17, 0, 0).expect("valid time"),
        };
        // 2 PM -- before 5 PM window
        let now = local(2026, 3, 14, 14, 0);
        assert!(!throttle.is_window_open(&now));
        assert!(throttle.is_throttled());
    }

    #[test]
    fn daily_after_window() {
        let throttle = BundleThrottle::Daily {
            delivery_time: NaiveTime::from_hms_opt(17, 0, 0).expect("valid time"),
        };
        // 6 PM -- after 5 PM window
        let now = local(2026, 3, 14, 18, 0);
        assert!(throttle.is_window_open(&now));
    }

    #[test]
    fn daily_exactly_at_window() {
        let throttle = BundleThrottle::Daily {
            delivery_time: NaiveTime::from_hms_opt(17, 0, 0).expect("valid time"),
        };
        let now = local(2026, 3, 14, 17, 0);
        assert!(throttle.is_window_open(&now));
    }

    #[test]
    fn weekly_on_delivery_day_before_time() {
        let throttle = BundleThrottle::Weekly {
            delivery_day: WeekdayWrapper(Weekday::Mon),
            delivery_time: NaiveTime::from_hms_opt(8, 0, 0).expect("valid time"),
        };
        // Monday 7 AM -- before 8 AM
        let now = local(2026, 3, 16, 7, 0); // March 16, 2026 is a Monday
        assert!(!throttle.is_window_open(&now));
    }

    #[test]
    fn weekly_on_delivery_day_after_time() {
        let throttle = BundleThrottle::Weekly {
            delivery_day: WeekdayWrapper(Weekday::Mon),
            delivery_time: NaiveTime::from_hms_opt(8, 0, 0).expect("valid time"),
        };
        // Monday 9 AM -- after 8 AM
        let now = local(2026, 3, 16, 9, 0);
        assert!(throttle.is_window_open(&now));
    }

    #[test]
    fn weekly_on_different_day_after_delivery() {
        let throttle = BundleThrottle::Weekly {
            delivery_day: WeekdayWrapper(Weekday::Mon),
            delivery_time: NaiveTime::from_hms_opt(8, 0, 0).expect("valid time"),
        };
        // Wednesday -- two days after Monday delivery
        let now = local(2026, 3, 18, 12, 0);
        assert!(throttle.is_window_open(&now));
    }

    #[test]
    fn weekly_on_day_before_delivery() {
        let throttle = BundleThrottle::Weekly {
            delivery_day: WeekdayWrapper(Weekday::Mon),
            delivery_time: NaiveTime::from_hms_opt(8, 0, 0).expect("valid time"),
        };
        // Sunday -- day before Monday delivery (should show last week's batch)
        let now = local(2026, 3, 15, 12, 0);
        assert!(throttle.is_window_open(&now));
    }

    #[test]
    fn serde_roundtrip_immediate() {
        let throttle = BundleThrottle::Immediate;
        let json = serde_json::to_string(&throttle).expect("serialize");
        let decoded: BundleThrottle = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(throttle, decoded);
    }

    #[test]
    fn serde_roundtrip_daily() {
        let throttle = BundleThrottle::Daily {
            delivery_time: NaiveTime::from_hms_opt(17, 0, 0).expect("valid time"),
        };
        let json = serde_json::to_string(&throttle).expect("serialize");
        let decoded: BundleThrottle = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(throttle, decoded);
    }

    #[test]
    fn serde_roundtrip_weekly() {
        let throttle = BundleThrottle::Weekly {
            delivery_day: WeekdayWrapper(Weekday::Mon),
            delivery_time: NaiveTime::from_hms_opt(8, 0, 0).expect("valid time"),
        };
        let json = serde_json::to_string(&throttle).expect("serialize");
        assert!(json.contains("monday"));
        let decoded: BundleThrottle = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(throttle, decoded);
    }

    #[test]
    fn next_window_daily_before_time() {
        let throttle = BundleThrottle::Daily {
            delivery_time: NaiveTime::from_hms_opt(17, 0, 0).expect("valid time"),
        };
        let now = local(2026, 3, 14, 14, 0);
        let next = throttle.next_window(&now).expect("should have next window");
        // Should be today at 5 PM
        assert_eq!(
            next.date_naive(),
            NaiveDate::from_ymd_opt(2026, 3, 14).expect("valid date")
        );
    }

    #[test]
    fn next_window_daily_after_time() {
        let throttle = BundleThrottle::Daily {
            delivery_time: NaiveTime::from_hms_opt(17, 0, 0).expect("valid time"),
        };
        let now = local(2026, 3, 14, 18, 0);
        let next = throttle.next_window(&now).expect("should have next window");
        // Should be tomorrow at 5 PM
        assert_eq!(
            next.date_naive(),
            NaiveDate::from_ymd_opt(2026, 3, 15).expect("valid date")
        );
    }

    #[test]
    fn days_since_same_day() {
        assert_eq!(days_since(Weekday::Mon, Weekday::Mon), 0);
    }

    #[test]
    fn days_since_next_day() {
        assert_eq!(days_since(Weekday::Mon, Weekday::Tue), 1);
    }

    #[test]
    fn days_since_wrap_around() {
        assert_eq!(days_since(Weekday::Sat, Weekday::Mon), 2);
    }

    #[test]
    fn default_is_immediate() {
        assert_eq!(BundleThrottle::default(), BundleThrottle::Immediate);
    }
}
