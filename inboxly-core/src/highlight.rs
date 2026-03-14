use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::id::ThreadId;

/// Smart extraction results from email body analysis.
/// Each variant represents a different type of actionable information.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Highlight {
    /// Package tracking number with carrier info.
    TrackingNumber {
        carrier: String,
        number: String,
        url: Option<String>,
    },
    /// Flight reservation details.
    Flight {
        airline: String,
        number: String,
        depart: DateTime<Utc>,
        arrive: DateTime<Utc>,
        gate: Option<String>,
    },
    /// Hotel reservation details.
    Hotel {
        name: String,
        checkin: NaiveDate,
        checkout: NaiveDate,
        confirmation: Option<String>,
    },
    /// Calendar event.
    Event {
        title: String,
        datetime: DateTime<Utc>,
        location: Option<String>,
    },
    /// Payment or financial transaction.
    Payment {
        amount: String,
        currency: String,
        from_or_to: String,
    },
}

impl Highlight {
    /// Returns the highlight type as a string for storage/display.
    pub fn highlight_type(&self) -> &'static str {
        match self {
            Self::TrackingNumber { .. } => "tracking",
            Self::Flight { .. } => "flight",
            Self::Hotel { .. } => "hotel",
            Self::Event { .. } => "event",
            Self::Payment { .. } => "payment",
        }
    }
}

/// Auto-grouped travel itinerary combining multiple travel highlights.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TripBundle {
    /// Destination label (from flight arrival city or hotel location).
    pub destination: String,
    /// Trip start date.
    pub start_date: NaiveDate,
    /// Trip end date.
    pub end_date: NaiveDate,
    /// Thread IDs containing the travel-related emails.
    pub threads: Vec<ThreadId>,
    /// Individual highlights that make up this trip.
    pub highlights: Vec<Highlight>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_type_names() {
        let h = Highlight::TrackingNumber {
            carrier: "UPS".into(),
            number: "1Z999AA10123456784".into(),
            url: Some("https://ups.com/track".into()),
        };
        assert_eq!(h.highlight_type(), "tracking");

        let h = Highlight::Flight {
            airline: "Air Canada".into(),
            number: "AC 123".into(),
            depart: Utc::now(),
            arrive: Utc::now() + chrono::Duration::hours(5),
            gate: Some("B42".into()),
        };
        assert_eq!(h.highlight_type(), "flight");
    }

    #[test]
    fn highlight_serde_roundtrip() {
        let h = Highlight::Hotel {
            name: "Marriott Downtown".into(),
            checkin: NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(),
            checkout: NaiveDate::from_ymd_opt(2026, 6, 18).unwrap(),
            confirmation: Some("ABC123".into()),
        };
        let json = serde_json::to_string(&h).unwrap();
        let back: Highlight = serde_json::from_str(&json).unwrap();
        assert_eq!(h, back);
    }

    #[test]
    fn trip_bundle_creation() {
        let trip = TripBundle {
            destination: "Toronto".into(),
            start_date: NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2026, 6, 20).unwrap(),
            threads: vec![ThreadId::new(), ThreadId::new()],
            highlights: vec![
                Highlight::Flight {
                    airline: "WestJet".into(),
                    number: "WS 456".into(),
                    depart: Utc::now(),
                    arrive: Utc::now() + chrono::Duration::hours(4),
                    gate: None,
                },
                Highlight::Hotel {
                    name: "Hilton".into(),
                    checkin: NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(),
                    checkout: NaiveDate::from_ymd_opt(2026, 6, 20).unwrap(),
                    confirmation: Some("XYZ789".into()),
                },
            ],
        };
        assert_eq!(trip.destination, "Toronto");
        assert_eq!(trip.highlights.len(), 2);
        assert_eq!(trip.threads.len(), 2);
    }

    #[test]
    fn payment_highlight() {
        let h = Highlight::Payment {
            amount: "42.99".into(),
            currency: "CAD".into(),
            from_or_to: "Amazon.ca".into(),
        };
        assert_eq!(h.highlight_type(), "payment");
    }

    #[test]
    fn event_highlight() {
        let h = Highlight::Event {
            title: "Team Standup".into(),
            datetime: Utc::now(),
            location: Some("Room 301".into()),
        };
        assert_eq!(h.highlight_type(), "event");
    }
}
