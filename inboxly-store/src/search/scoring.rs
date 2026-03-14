/// Half-life for recency decay in days.
///
/// An email `RECENCY_HALF_LIFE_DAYS` old gets a boost of ~1.5x (halfway between
/// max boost and no boost). Newer emails get up to 2x; much older emails approach 1x.
const RECENCY_HALF_LIFE_DAYS: f64 = 60.0;

/// Maximum recency boost multiplier for the newest emails.
const MAX_RECENCY_BOOST: f32 = 2.0;

/// Minimum recency boost multiplier (floor — very old emails).
const MIN_RECENCY_BOOST: f32 = 1.0;

/// Compute the recency boost factor for a given timestamp.
///
/// Returns a multiplier between MIN_RECENCY_BOOST and MAX_RECENCY_BOOST.
///
/// Formula: `boost = MIN + (MAX - MIN) * exp(-age_days / half_life)`
pub fn recency_boost_factor(email_timestamp_secs: i64) -> f32 {
    let now_secs = chrono::Utc::now().timestamp();
    let age_secs = (now_secs - email_timestamp_secs).max(0) as f64;
    let age_days = age_secs / 86400.0;

    let decay = (-age_days / RECENCY_HALF_LIFE_DAYS).exp() as f32;
    MIN_RECENCY_BOOST + (MAX_RECENCY_BOOST - MIN_RECENCY_BOOST) * decay
}
