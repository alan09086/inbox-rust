# M25: Highlights + Trips + Multi-Account + Polish — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan.

**Goal:** Deliver the final v1 features — smart email extraction (highlights), trip bundle assembly, multi-account support, Inbox Zero Sun, and all UI animations.

**Crates:** `inboxly-extract`, `inboxly-imap`, `inboxly-store`, `inboxly-ui`, `inboxly-core`, `inboxly` (binary)

**Branch:** `m25-highlights-polish`

**Prereqs:** M1-M24 complete (workspace, core types, store, IMAP sync, bundler, snooze, UI shell, theme, inbox feed, bundles, done/pin/sweep, swipe, snooze picker, reminders/FAB, compose/SMTP, search)

**Spec ref:** Design spec §Highlights / Smart Extraction (lines 462-479), §Trip Bundle Assembly (lines 476-479), §Multi-account (lines 254, 319-322, 682-683), §Inbox Zero Sun (line 530, 685), §Animations (lines 544-548), §InboxItem::TripBundle (line 194), §Highlight enum (lines 219-225)

**Tech Stack:** Rust, regex, scraper, ical, iced

---

> **⚠ Plan Correction (post-M13 review):** `SyncManager` already exists in `inboxly-imap/src/sync_manager.rs` (built in M9) with `register()`, `stop()`, `stop_all()`, and `running_accounts()` methods. Multi-account sync orchestration is already implemented. **M25's multi-account work should focus on: (a) account CRUD in Store, (b) account switcher UI, (c) per-account nav drawer sections — not rebuilding SyncManager.**

## Sub-section Index

| Section | Tasks | Scope |
|---------|-------|-------|
| A. Highlights — `inboxly-extract` crate | 1-8 | Crate setup, all 5 extractors, pipeline, tests |
| B. Store integration | 9-11 | SQLite highlights table, extraction on ingest, query API |
| C. Trip Bundles | 12-14 | Detection algorithm, TripBundle assembly, store layer |
| D. Multi-Account | 15-19 | Account CRUD, per-account sync, switcher UI, unified inbox |
| E. Inbox Zero Sun | 20-21 | Canvas widget, integration into inbox feed |
| F. Animations | 22-27 | Bundle expand/collapse, sweep cascade, FAB speed dial, toolbar crossfade, swipe commit, general spring/ease helpers |
| G. Highlight + Trip UI | 28-30 | Highlight cards, trip bundle card, feed integration |
| H. Integration tests | 31-32 | End-to-end extraction, full app smoke test |

---

## A. Highlights — `inboxly-extract` Crate

### Task 1 — Crate setup and dependencies

**Files:**
- Modify: `/mnt/TempNVME/projects/inbox-rust/inboxly-extract/Cargo.toml`
- Modify: `/mnt/TempNVME/projects/inbox-rust/inboxly-extract/src/lib.rs`

Set up `inboxly-extract` with its dependencies and public module structure.

**`Cargo.toml`:**

```toml
[package]
name = "inboxly-extract"
version.workspace = true
edition = "2021"

[dependencies]
inboxly-core = { path = "../inboxly-core" }
regex = "1"
scraper = "0.22"
ical = "0.11"
chrono = { version = "0.4", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
once_cell = "1"
log = "0.4"

[dev-dependencies]
pretty_assertions = "1"
```

**`src/lib.rs`:**

```rust
//! Email body parsing and smart extraction for highlights.
//!
//! Extracts tracking numbers, flights, hotels, events, and payments
//! from email bodies and headers using regex and HTML parsing.

mod error;
mod extractors;
mod pipeline;

pub use error::ExtractError;
pub use pipeline::{extract_highlights, ExtractInput};

// Re-export the Highlight type from core
pub use inboxly_core::Highlight;
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-extract
```

**Commit:** `feat(extract): scaffold inboxly-extract crate with dependencies`

---

### Task 2 — Error type and ExtractInput

**Files:**
- Create: `/mnt/TempNVME/projects/inbox-rust/inboxly-extract/src/error.rs`
- Create: `/mnt/TempNVME/projects/inbox-rust/inboxly-extract/src/pipeline.rs` (partial — input type only)

**`error.rs`:**

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExtractError {
    #[error("HTML parse error: {0}")]
    HtmlParse(String),

    #[error("iCal parse error: {0}")]
    IcalParse(String),

    #[error("regex error: {0}")]
    Regex(#[from] regex::Error),

    #[error("date parse error: {0}")]
    DateParse(String),
}
```

**`pipeline.rs` (initial):**

```rust
use std::collections::HashMap;

/// Input data for the extraction pipeline.
/// Constructed from an email's content and headers.
pub struct ExtractInput {
    /// Plain-text body (if available).
    pub body_text: Option<String>,
    /// HTML body (if available).
    pub body_html: Option<String>,
    /// Email headers (key → value). Keys are canonical-cased.
    pub headers: HashMap<String, String>,
    /// Subject line.
    pub subject: String,
    /// Attachment filenames with their raw bytes (for .ics parsing).
    pub attachments: Vec<(String, Vec<u8>)>,
}
```

**Commit:** `feat(extract): add error type and extraction input struct`

---

### Task 3 — TrackingNumber extractor

**Files:**
- Create: `/mnt/TempNVME/projects/inbox-rust/inboxly-extract/src/extractors/mod.rs`
- Create: `/mnt/TempNVME/projects/inbox-rust/inboxly-extract/src/extractors/tracking.rs`

**`extractors/mod.rs`:**

```rust
pub mod tracking;
pub mod flight;
pub mod hotel;
pub mod event;
pub mod payment;

use inboxly_core::Highlight;
use crate::{ExtractInput, ExtractError};

/// Trait implemented by each extractor.
pub trait Extractor: Send + Sync {
    /// Extract highlights from the given input.
    fn extract(&self, input: &ExtractInput) -> Result<Vec<Highlight>, ExtractError>;
}
```

**`extractors/tracking.rs`:**

Implement `TrackingExtractor` that detects tracking numbers from major carriers.

```rust
use once_cell::sync::Lazy;
use regex::Regex;
use inboxly_core::Highlight;
use super::{Extractor, ExtractInput, ExtractError};

pub struct TrackingExtractor;

/// Carrier-specific regex patterns.
/// Each tuple: (carrier_name, regex, tracking_url_template with {number} placeholder)
static TRACKING_PATTERNS: Lazy<Vec<(&str, Regex, &str)>> = Lazy::new(|| vec![
    (
        "UPS",
        Regex::new(r"\b1Z[0-9A-Z]{16}\b").unwrap(),
        "https://www.ups.com/track?tracknum={number}",
    ),
    (
        "FedEx",
        // FedEx: 12, 15, 20, or 22 digits
        Regex::new(r"\b(?:\d{12}|\d{15}|\d{20}|\d{22})\b").unwrap(),
        "https://www.fedex.com/fedextrack/?trknbr={number}",
    ),
    (
        "USPS",
        // USPS: 20-22 digits, or starts with specific prefixes
        Regex::new(r"\b(?:(?:94|93|92|94)\d{18,20}|[A-Z]{2}\d{9}US)\b").unwrap(),
        "https://tools.usps.com/go/TrackConfirmAction?tLabels={number}",
    ),
    (
        "Canada Post",
        Regex::new(r"\b\d{16}\b").unwrap(),
        "https://www.canadapost-postescanada.ca/track-reperage/en#/search?searchFor={number}",
    ),
    (
        "DHL",
        // DHL: 10-11 digits or JD + 18 digits
        Regex::new(r"\b(?:\d{10,11}|JD\d{18})\b").unwrap(),
        "https://www.dhl.com/en/express/tracking.html?AWB={number}",
    ),
]);

impl Extractor for TrackingExtractor {
    fn extract(&self, input: &ExtractInput) -> Result<Vec<Highlight>, ExtractError> {
        let mut highlights = Vec::new();

        // 1. Check X-Tracking-Number header first (most reliable signal)
        if let Some(tracking_header) = input.headers.get("X-Tracking-Number")
            .or_else(|| input.headers.get("x-tracking-number"))
        {
            let number = tracking_header.trim().to_string();
            let (carrier, url) = identify_carrier(&number);
            highlights.push(Highlight::TrackingNumber {
                carrier: carrier.to_string(),
                number: number.clone(),
                url,
            });
        }

        // 2. Scan body text for tracking patterns
        let text = combine_text(input);
        if text.is_empty() {
            return Ok(highlights);
        }

        // Only search if the email looks like a shipping notification
        let shipping_keywords = ["tracking", "shipped", "delivery", "shipment",
                                  "package", "order shipped", "out for delivery",
                                  "in transit"];
        let lower_text = text.to_lowercase();
        let has_shipping_context = shipping_keywords.iter()
            .any(|kw| lower_text.contains(kw))
            || input.subject.to_lowercase().contains("ship")
            || input.subject.to_lowercase().contains("tracking")
            || input.subject.to_lowercase().contains("delivery");

        if !has_shipping_context {
            return Ok(highlights);
        }

        for (carrier, pattern, url_template) in TRACKING_PATTERNS.iter() {
            for cap in pattern.find_iter(&text) {
                let number = cap.as_str().to_string();
                // Skip numbers that are too short to be meaningful
                // (reduces false positives from Canada Post 16-digit pattern)
                if carrier == &"Canada Post" && !lower_text.contains("canada post")
                    && !lower_text.contains("canadapost") {
                    continue;
                }
                let url = url_template.replace("{number}", &number);
                let h = Highlight::TrackingNumber {
                    carrier: carrier.to_string(),
                    number: number.clone(),
                    url,
                };
                if !highlights.iter().any(|existing| match existing {
                    Highlight::TrackingNumber { number: n, .. } => n == &number,
                    _ => false,
                }) {
                    highlights.push(h);
                }
            }
        }

        Ok(highlights)
    }
}

/// Identify carrier from a tracking number (best-effort).
fn identify_carrier(number: &str) -> (&'static str, String) {
    for (carrier, pattern, url_template) in TRACKING_PATTERNS.iter() {
        if pattern.is_match(number) {
            return (carrier, url_template.replace("{number}", number));
        }
    }
    ("Unknown", String::new())
}

/// Combine available text from body_text and stripped body_html.
fn combine_text(input: &ExtractInput) -> String {
    let mut parts = Vec::new();
    if let Some(ref text) = input.body_text {
        parts.push(text.clone());
    }
    if let Some(ref html) = input.body_html {
        // Strip HTML tags for simple text extraction
        // scraper is used for structured extraction; here we just need raw text
        if let Ok(doc) = scraper::Html::parse_document(html).root_element()
            .text().collect::<String>().len().checked_add(0) // always succeeds
        {
            let doc = scraper::Html::parse_document(html);
            let text: String = doc.root_element().text().collect();
            parts.push(text);
        }
    }
    parts.join("\n")
}
```

**Note on `combine_text`:** Simplify to just:

```rust
fn combine_text(input: &ExtractInput) -> String {
    let mut parts = Vec::new();
    if let Some(ref text) = input.body_text {
        parts.push(text.clone());
    }
    if let Some(ref html) = input.body_html {
        let doc = scraper::Html::parse_document(html);
        let text: String = doc.root_element().text().collect();
        parts.push(text);
    }
    parts.join("\n")
}
```

Move `combine_text` to a shared `extractors/util.rs` since multiple extractors need it.

**Tests** (inline `#[cfg(test)]`):

1. UPS tracking number `1Z999AA10123456784` detected in body with shipping keywords
2. FedEx 12-digit number detected
3. USPS 20-digit number detected
4. `X-Tracking-Number` header extracts directly without body scan
5. Email without shipping keywords returns empty (no false positives)
6. Duplicate tracking numbers deduplicated
7. Multiple carriers in one email all detected

**Commit:** `feat(extract): add tracking number extractor with carrier-specific patterns`

---

### Task 4 — Flight extractor

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-extract/src/extractors/flight.rs`

Detect flight numbers and parse departure/arrival information from airline confirmation emails.

```rust
use once_cell::sync::Lazy;
use regex::Regex;
use inboxly_core::Highlight;
use super::{Extractor, ExtractInput, ExtractError};
use super::util::combine_text;

pub struct FlightExtractor;

/// IATA airline code (2 chars) + flight number (1-4 digits), with optional space.
/// Examples: AC 123, WS456, UA 1234, DL12
static FLIGHT_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b([A-Z]{2})\s?(\d{1,4})\b").unwrap()
});

/// Common airline codes to reduce false positives.
/// Only match flight numbers when the code is a known airline.
static KNOWN_AIRLINES: Lazy<std::collections::HashSet<&str>> = Lazy::new(|| {
    [
        "AC", "WS", "TS", "PD", // Canadian
        "AA", "UA", "DL", "WN", "AS", "B6", "NK", "F9", "HA", "G4", // US
        "BA", "LH", "AF", "KL", "IB", "AZ", "SK", "AY", "TP", "EI", // European
        "QF", "NZ", "SQ", "CX", "TG", "MH", "GA", "PR", // Asia-Pacific
        "EK", "QR", "EY", "TK", "SV", // Middle East
        "ET", "SA", "KQ", // Africa
        "LA", "AV", "CM", "AR", // Latin America
        "SW", "WG", "FR", "U2", "W6", // Low-cost
    ].into_iter().collect()
});

/// Patterns to extract departure/arrival cities and times.
static DEPART_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:depart(?:ure|ing|s)?|from)\s*:?\s*(.+?)(?:\n|$)").unwrap()
});

static ARRIVE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:arriv(?:al|ing|es?)?|to)\s*:?\s*(.+?)(?:\n|$)").unwrap()
});

static GATE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)gate\s*:?\s*([A-Z]?\d{1,3}[A-Z]?)").unwrap()
});

impl Extractor for FlightExtractor {
    fn extract(&self, input: &ExtractInput) -> Result<Vec<Highlight>, ExtractError> {
        let text = combine_text(input);
        if text.is_empty() {
            return Ok(Vec::new());
        }

        // Check for flight context
        let lower = text.to_lowercase();
        let subject_lower = input.subject.to_lowercase();
        let has_flight_context = ["flight", "boarding", "itinerary", "confirmation",
                                   "e-ticket", "booking", "reservation", "check-in"]
            .iter()
            .any(|kw| lower.contains(kw) || subject_lower.contains(kw));

        if !has_flight_context {
            return Ok(Vec::new());
        }

        let mut highlights = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for cap in FLIGHT_REGEX.captures_iter(&text) {
            let airline = cap.get(1).unwrap().as_str();
            let number = cap.get(2).unwrap().as_str();

            if !KNOWN_AIRLINES.contains(airline) {
                continue;
            }

            let flight_key = format!("{}{}", airline, number);
            if !seen.insert(flight_key.clone()) {
                continue;
            }

            // Try to extract departure/arrival/gate from nearby text
            let depart = DEPART_REGEX.captures(&text)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().trim().to_string());

            let arrive = ARRIVE_REGEX.captures(&text)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().trim().to_string());

            let gate = GATE_REGEX.captures(&text)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().trim().to_string());

            highlights.push(Highlight::Flight {
                airline: airline.to_string(),
                number: format!("{} {}", airline, number),
                depart,
                arrive,
                gate,
            });
        }

        Ok(highlights)
    }
}
```

**Tests:**

1. `AC 123` in airline confirmation email detected
2. `WS456` (no space) detected
3. Unknown airline code `ZZ 999` not matched (false positive prevention)
4. Departure and arrival lines parsed when present
5. Gate extracted from "Gate: B42"
6. No matches in non-flight email (no flight context keywords)
7. Multiple flights in one itinerary email all detected
8. Duplicate flight numbers deduplicated

**Commit:** `feat(extract): add flight extractor with airline code validation`

---

### Task 5 — Hotel extractor

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-extract/src/extractors/hotel.rs`

Detect hotel reservations — confirmation numbers, check-in/checkout dates, hotel names.

```rust
use once_cell::sync::Lazy;
use regex::Regex;
use inboxly_core::Highlight;
use super::{Extractor, ExtractInput, ExtractError};
use super::util::combine_text;

pub struct HotelExtractor;

/// Confirmation number patterns (various formats from major booking sites).
static CONFIRMATION_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:confirmation|booking|reservation)\s*(?:number|#|no\.?|code|id)\s*:?\s*([A-Z0-9]{4,20})").unwrap()
});

/// Check-in date patterns.
static CHECKIN_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)check[\s-]?in\s*:?\s*(.+?)(?:\n|$|check)").unwrap()
});

/// Check-out date patterns.
static CHECKOUT_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)check[\s-]?out\s*:?\s*(.+?)(?:\n|$)").unwrap()
});

/// Hotel name — look for "Hotel: <name>" or known booking platform patterns.
static HOTEL_NAME_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:hotel|property|accommodation|stay at|staying at)\s*:?\s*(.+?)(?:\n|$)").unwrap()
});

impl Extractor for HotelExtractor {
    fn extract(&self, input: &ExtractInput) -> Result<Vec<Highlight>, ExtractError> {
        let text = combine_text(input);
        if text.is_empty() {
            return Ok(Vec::new());
        }

        let lower = text.to_lowercase();
        let subject_lower = input.subject.to_lowercase();
        let has_hotel_context = ["hotel", "booking", "reservation", "check-in",
                                  "check-out", "accommodation", "airbnb", "stay",
                                  "booking.com", "expedia", "hotels.com"]
            .iter()
            .any(|kw| lower.contains(kw) || subject_lower.contains(kw));

        if !has_hotel_context {
            return Ok(Vec::new());
        }

        let confirmation = CONFIRMATION_REGEX.captures(&text)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_string());

        // Only produce a highlight if we found a confirmation number
        // (without it, too many false positives)
        let confirmation = match confirmation {
            Some(c) => c,
            None => return Ok(Vec::new()),
        };

        let checkin = CHECKIN_REGEX.captures(&text)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_string());

        let checkout = CHECKOUT_REGEX.captures(&text)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_string());

        let name = HOTEL_NAME_REGEX.captures(&text)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_else(|| "Hotel".to_string());

        Ok(vec![Highlight::Hotel {
            name,
            checkin,
            checkout,
            confirmation,
        }])
    }
}
```

**Tests:**

1. Booking.com confirmation email with "Confirmation Number: ABC12345" detected
2. Check-in and check-out dates parsed
3. Hotel name extracted from "Hotel: Grand Hyatt"
4. No match without hotel context keywords
5. No match when confirmation number is missing
6. Airbnb reservation format detected

**Commit:** `feat(extract): add hotel extractor with confirmation number detection`

---

### Task 6 — Event extractor

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-extract/src/extractors/event.rs`

Parse `.ics` attachments and detect event invitations from email body patterns.

```rust
use inboxly_core::Highlight;
use super::{Extractor, ExtractInput, ExtractError};
use super::util::combine_text;
use once_cell::sync::Lazy;
use regex::Regex;

pub struct EventExtractor;

/// Regex for "Date: ...", "When: ...", "Time: ..." patterns in invitation emails.
static DATE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:date|when|time)\s*:?\s*(.+?)(?:\n|$)").unwrap()
});

/// Regex for "Where: ...", "Location: ...", "Venue: ..." patterns.
static LOCATION_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:where|location|venue|place|address)\s*:?\s*(.+?)(?:\n|$)").unwrap()
});

impl Extractor for EventExtractor {
    fn extract(&self, input: &ExtractInput) -> Result<Vec<Highlight>, ExtractError> {
        let mut highlights = Vec::new();

        // 1. Parse .ics attachments (most reliable source)
        for (filename, data) in &input.attachments {
            if filename.ends_with(".ics") || filename.ends_with(".ical") {
                match parse_ical_attachment(data) {
                    Ok(mut events) => highlights.append(&mut events),
                    Err(e) => {
                        log::warn!("Failed to parse iCal attachment {}: {}", filename, e);
                    }
                }
            }
        }

        // 2. If no .ics attachments, try regex extraction from body
        if highlights.is_empty() {
            let text = combine_text(input);
            if text.is_empty() {
                return Ok(highlights);
            }

            let lower = text.to_lowercase();
            let subject_lower = input.subject.to_lowercase();
            let has_event_context = ["invitation", "invite", "event", "rsvp",
                                      "attend", "calendar", "meeting", "webinar",
                                      "conference"]
                .iter()
                .any(|kw| lower.contains(kw) || subject_lower.contains(kw));

            if !has_event_context {
                return Ok(highlights);
            }

            let datetime = DATE_REGEX.captures(&text)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().trim().to_string());

            let location = LOCATION_REGEX.captures(&text)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().trim().to_string());

            // Use subject as event title if we found a datetime
            if datetime.is_some() {
                highlights.push(Highlight::Event {
                    title: input.subject.clone(),
                    datetime,
                    location,
                });
            }
        }

        Ok(highlights)
    }
}

/// Parse an iCalendar (.ics) attachment and extract VEVENT components.
fn parse_ical_attachment(data: &[u8]) -> Result<Vec<Highlight>, ExtractError> {
    let text = std::str::from_utf8(data)
        .map_err(|e| ExtractError::IcalParse(format!("invalid UTF-8: {}", e)))?;

    let reader = ical::IcalParser::new(text.as_bytes());
    let mut highlights = Vec::new();

    for calendar in reader {
        let calendar = calendar
            .map_err(|e| ExtractError::IcalParse(format!("{}", e)))?;

        for event in calendar.events {
            let mut title = None;
            let mut datetime = None;
            let mut location = None;

            for prop in &event.properties {
                match prop.name.as_str() {
                    "SUMMARY" => title = prop.value.clone(),
                    "DTSTART" => datetime = prop.value.clone(),
                    "LOCATION" => location = prop.value.clone(),
                    _ => {}
                }
            }

            if let Some(title) = title {
                highlights.push(Highlight::Event {
                    title,
                    datetime,
                    location,
                });
            }
        }
    }

    Ok(highlights)
}
```

**Tests:**

1. `.ics` attachment with VEVENT → event extracted with title, datetime, location
2. Multiple VEVENTs in one `.ics` → multiple highlights
3. Email body with "Date: March 20, 2026" and "Where: Conference Room A" → event highlight
4. No event context keywords → empty results
5. Malformed `.ics` → logged warning, no crash, empty result
6. Non-UTF-8 `.ics` → error handled gracefully

**Commit:** `feat(extract): add event extractor with iCal and regex parsing`

---

### Task 7 — Payment extractor

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-extract/src/extractors/payment.rs`

Detect receipts, invoices, and payment confirmations with currency amounts.

```rust
use once_cell::sync::Lazy;
use regex::Regex;
use inboxly_core::Highlight;
use super::{Extractor, ExtractInput, ExtractError};
use super::util::combine_text;

pub struct PaymentExtractor;

/// Currency amount patterns: $123.45, CAD 50.00, EUR 100, USD 25.99, etc.
static AMOUNT_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?:(?P<symbol>[$€£¥])(?P<amt1>\d{1,3}(?:,\d{3})*(?:\.\d{2})?))|(?:(?P<code>USD|CAD|EUR|GBP|AUD|JPY|CHF|INR|BRL|MXN)\s*(?P<amt2>\d{1,3}(?:,\d{3})*(?:\.\d{2})?))").unwrap()
});

/// Map currency symbols to codes.
fn symbol_to_code(symbol: &str) -> &'static str {
    match symbol {
        "$" => "USD",
        "€" => "EUR",
        "£" => "GBP",
        "¥" => "JPY",
        _ => "USD",
    }
}

impl Extractor for PaymentExtractor {
    fn extract(&self, input: &ExtractInput) -> Result<Vec<Highlight>, ExtractError> {
        let text = combine_text(input);
        if text.is_empty() {
            return Ok(Vec::new());
        }

        let lower = text.to_lowercase();
        let subject_lower = input.subject.to_lowercase();
        let has_payment_context = ["receipt", "invoice", "payment", "paid",
                                    "transaction", "purchase", "order confirmation",
                                    "billing", "charged", "total"]
            .iter()
            .any(|kw| lower.contains(kw) || subject_lower.contains(kw));

        if !has_payment_context {
            return Ok(Vec::new());
        }

        let mut highlights = Vec::new();

        // Try to determine who the payment is from/to
        let from_or_to = extract_payment_party(input);

        // Find the largest amount (likely the total)
        let mut largest_amount: Option<(String, String)> = None; // (amount_str, currency)
        let mut largest_value: f64 = 0.0;

        for cap in AMOUNT_REGEX.captures_iter(&text) {
            let (amount_str, currency) = if let Some(sym) = cap.name("symbol") {
                let amt = cap.name("amt1").unwrap().as_str();
                (amt.to_string(), symbol_to_code(sym.as_str()).to_string())
            } else {
                let code = cap.name("code").unwrap().as_str();
                let amt = cap.name("amt2").unwrap().as_str();
                (amt.to_string(), code.to_string())
            };

            let value: f64 = amount_str.replace(',', "").parse().unwrap_or(0.0);
            if value > largest_value {
                largest_value = value;
                largest_amount = Some((amount_str, currency));
            }
        }

        if let Some((amount, currency)) = largest_amount {
            highlights.push(Highlight::Payment {
                amount,
                currency,
                from_or_to,
            });
        }

        Ok(highlights)
    }
}

/// Try to determine the payment party from the From header or body.
fn extract_payment_party(input: &ExtractInput) -> Option<String> {
    // Use the sender's display name or domain as the payment party
    input.headers.get("From")
        .or_else(|| input.headers.get("from"))
        .map(|from| {
            // Extract display name portion: "PayPal <noreply@paypal.com>" → "PayPal"
            if let Some(idx) = from.find('<') {
                from[..idx].trim().trim_matches('"').to_string()
            } else {
                from.clone()
            }
        })
        .filter(|s| !s.is_empty())
}
```

**Tests:**

1. `$123.45` in receipt email → Payment highlight with USD
2. `CAD 50.00` detected with correct currency code
3. `€100` detected as EUR
4. Largest amount used as the total (not line items)
5. Payment party extracted from From header display name
6. No payment context → empty results
7. Comma-separated amounts like `$1,234.56` parsed correctly

**Commit:** `feat(extract): add payment extractor with currency amount detection`

---

### Task 8 — Extraction pipeline and shared utilities

**Files:**
- Create: `/mnt/TempNVME/projects/inbox-rust/inboxly-extract/src/extractors/util.rs`
- Complete: `/mnt/TempNVME/projects/inbox-rust/inboxly-extract/src/pipeline.rs`

**`extractors/util.rs`:**

```rust
use crate::pipeline::ExtractInput;

/// Combine available text from body_text and stripped body_html.
/// HTML is parsed with scraper to extract text content.
pub fn combine_text(input: &ExtractInput) -> String {
    let mut parts = Vec::new();
    if let Some(ref text) = input.body_text {
        parts.push(text.clone());
    }
    if let Some(ref html) = input.body_html {
        let doc = scraper::Html::parse_document(html);
        let text: String = doc.root_element().text().collect();
        parts.push(text);
    }
    parts.join("\n")
}
```

**`pipeline.rs` (complete):**

```rust
use std::collections::HashMap;
use inboxly_core::Highlight;
use crate::ExtractError;
use crate::extractors::{Extractor, tracking, flight, hotel, event, payment};

/// Input data for the extraction pipeline.
pub struct ExtractInput {
    pub body_text: Option<String>,
    pub body_html: Option<String>,
    pub headers: HashMap<String, String>,
    pub subject: String,
    pub attachments: Vec<(String, Vec<u8>)>,
}

/// Run all extractors against the given email content.
/// Returns a combined Vec<Highlight> from all extractors.
/// Individual extractor errors are logged but do not fail the pipeline.
pub fn extract_highlights(input: &ExtractInput) -> Vec<Highlight> {
    let extractors: Vec<Box<dyn Extractor>> = vec![
        Box::new(tracking::TrackingExtractor),
        Box::new(flight::FlightExtractor),
        Box::new(hotel::HotelExtractor),
        Box::new(event::EventExtractor),
        Box::new(payment::PaymentExtractor),
    ];

    let mut all_highlights = Vec::new();

    for extractor in &extractors {
        match extractor.extract(input) {
            Ok(mut highlights) => all_highlights.append(&mut highlights),
            Err(e) => {
                log::warn!("Extractor failed: {}", e);
            }
        }
    }

    all_highlights
}
```

**Tests** (pipeline-level integration tests in `inboxly-extract/tests/pipeline_integration.rs`):

1. Amazon order confirmation → TrackingNumber + Payment highlights
2. Airline booking email → Flight + (possibly) Hotel highlights
3. Calendar invitation with .ics → Event highlight
4. Plain personal email → empty highlights (no false positives)
5. Email with both tracking number and receipt → both extracted
6. Empty input → empty highlights, no errors
7. HTML-only body (no plaintext) → text extracted from HTML and processed

**Commit:** `feat(extract): complete extraction pipeline with all 5 extractors`

---

## B. Store Integration

### Task 9 — Highlights SQLite schema migration

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/migrations.rs` (modify existing)

The `highlights` table already exists in the M3 schema (from the spec: `highlights — thread_id, highlight_type, data_json`). This task ensures the table is usable and adds query functions.

If the highlights table was defined in the M3 initial schema, verify it has:

```sql
CREATE TABLE IF NOT EXISTS highlights (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    email_id TEXT NOT NULL REFERENCES emails(id) ON DELETE CASCADE,
    highlight_type TEXT NOT NULL,  -- 'tracking', 'flight', 'hotel', 'event', 'payment'
    data_json TEXT NOT NULL,       -- serialized Highlight variant fields
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    UNIQUE(email_id, highlight_type, data_json)  -- prevent duplicate highlights per email
);
CREATE INDEX IF NOT EXISTS idx_highlights_thread ON highlights(thread_id);
CREATE INDEX IF NOT EXISTS idx_highlights_type ON highlights(highlight_type);
```

If the M3 table is simpler (only `thread_id, highlight_type, data_json`), add a migration to bring it to this schema. Key additions:
- `email_id` column — granular tracking of which email produced the highlight
- `UNIQUE` constraint — idempotent re-extraction
- `id` auto-increment PK for deletion

**Commit:** `feat(store): ensure highlights table schema supports extraction pipeline`

---

### Task 10 — Highlight store API

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/highlights.rs` (new)

CRUD operations for highlights in SQLite.

```rust
use rusqlite::{Connection, params};
use inboxly_core::Highlight;

/// Insert highlights for an email. Idempotent — duplicates are ignored via
/// the UNIQUE constraint (ON CONFLICT IGNORE).
pub fn insert_highlights(
    conn: &Connection,
    thread_id: &str,
    email_id: &str,
    highlights: &[Highlight],
) -> Result<usize, StoreError> {
    let mut count = 0;
    for h in highlights {
        let (highlight_type, data_json) = serialize_highlight(h);
        let result = conn.execute(
            "INSERT OR IGNORE INTO highlights (thread_id, email_id, highlight_type, data_json)
             VALUES (?1, ?2, ?3, ?4)",
            params![thread_id, email_id, highlight_type, data_json],
        )?;
        count += result;
    }
    Ok(count)
}

/// Get all highlights for a thread, ordered by creation time.
pub fn get_highlights_for_thread(
    conn: &Connection,
    thread_id: &str,
) -> Result<Vec<Highlight>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT highlight_type, data_json FROM highlights
         WHERE thread_id = ?1 ORDER BY created_at ASC"
    )?;
    let rows = stmt.query_map(params![thread_id], |row| {
        let ht: String = row.get(0)?;
        let data: String = row.get(1)?;
        Ok((ht, data))
    })?;
    let mut highlights = Vec::new();
    for row in rows {
        let (ht, data) = row?;
        if let Some(h) = deserialize_highlight(&ht, &data) {
            highlights.push(h);
        }
    }
    Ok(highlights)
}

/// Get all highlights of a specific type across all threads (for trip assembly).
pub fn get_highlights_by_type(
    conn: &Connection,
    highlight_type: &str,
) -> Result<Vec<(String, Highlight)>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT thread_id, data_json FROM highlights
         WHERE highlight_type = ?1 ORDER BY created_at ASC"
    )?;
    let rows = stmt.query_map(params![highlight_type], |row| {
        let tid: String = row.get(0)?;
        let data: String = row.get(1)?;
        Ok((tid, data))
    })?;
    let mut results = Vec::new();
    for row in rows {
        let (tid, data) = row?;
        if let Some(h) = deserialize_highlight(highlight_type, &data) {
            results.push((tid, h));
        }
    }
    Ok(results)
}

/// Delete all highlights for an email (used when re-extracting).
pub fn delete_highlights_for_email(
    conn: &Connection,
    email_id: &str,
) -> Result<usize, StoreError> {
    Ok(conn.execute(
        "DELETE FROM highlights WHERE email_id = ?1",
        params![email_id],
    )?)
}

/// Serialize a Highlight variant to (type_string, json_data).
fn serialize_highlight(h: &Highlight) -> (String, String) {
    match h {
        Highlight::TrackingNumber { .. } => ("tracking".into(), serde_json::to_string(h).unwrap()),
        Highlight::Flight { .. } => ("flight".into(), serde_json::to_string(h).unwrap()),
        Highlight::Hotel { .. } => ("hotel".into(), serde_json::to_string(h).unwrap()),
        Highlight::Event { .. } => ("event".into(), serde_json::to_string(h).unwrap()),
        Highlight::Payment { .. } => ("payment".into(), serde_json::to_string(h).unwrap()),
    }
}

/// Deserialize a Highlight from type string + JSON data.
fn deserialize_highlight(highlight_type: &str, data: &str) -> Option<Highlight> {
    serde_json::from_str(data).ok()
}
```

**Tests:**

1. Insert + retrieve highlights for a thread
2. Duplicate insertion is idempotent (returns 0 on second insert)
3. Delete highlights for email removes correct rows
4. Get by type returns correct subset
5. Round-trip serialize/deserialize for each Highlight variant

**Commit:** `feat(store): add highlight CRUD operations`

---

### Task 11 — Hook extraction into email ingest pipeline

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/store.rs` (modify — or wherever email body download completes)

Add `inboxly-extract` as a dependency of `inboxly-store`.

**`inboxly-store/Cargo.toml`** — add:

```toml
inboxly-extract = { path = "../inboxly-extract" }
```

At the point where email bodies are downloaded and stored (end of initial sync Phase 2, or on-demand body fetch), run extraction:

```rust
use inboxly_extract::{extract_highlights, ExtractInput};

/// Called after an email body is downloaded and written to Maildir.
/// Runs the extraction pipeline and stores highlights in SQLite.
pub fn extract_and_store_highlights(
    conn: &Connection,
    email_id: &str,
    thread_id: &str,
    email_content: &EmailContent,
) -> Result<(), StoreError> {
    let input = ExtractInput {
        body_text: email_content.body_text.clone(),
        body_html: email_content.body_html.clone(),
        headers: email_content.headers.clone(),
        subject: email_content.headers.get("Subject")
            .cloned()
            .unwrap_or_default(),
        attachments: email_content.attachments.iter()
            .map(|a| (a.filename.clone(), a.data.clone()))
            .collect(),
    };

    let highlights = extract_highlights(&input);

    if !highlights.is_empty() {
        // Delete any existing highlights for this email (re-extraction safe)
        delete_highlights_for_email(conn, email_id)?;
        insert_highlights(conn, thread_id, email_id, &highlights)?;
        log::info!("Extracted {} highlights from email {}", highlights.len(), email_id);
    }

    Ok(())
}
```

Call this from:
1. The Phase 2 body download loop (M8 integration point)
2. The on-demand body fetch path (when user opens an email whose body hasn't been downloaded)
3. Incremental sync when new email bodies arrive

**Tests:**

1. Insert email with body containing tracking number → highlight appears in DB
2. Re-extraction (call twice) → no duplicates
3. Email with no extractable content → no highlights stored

**Commit:** `feat(store): integrate extraction pipeline into email ingest`

---

## C. Trip Bundles

### Task 12 — Trip detection algorithm

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-extract/src/trips.rs` (new)

Group travel-related highlights (flights, hotels) by overlapping date ranges into trip bundles.

```rust
use chrono::{NaiveDate, Duration};
use inboxly_core::{Highlight, TripBundle, ThreadId};

/// A travel highlight with its associated thread and parsed date range.
#[derive(Debug, Clone)]
pub struct TravelItem {
    pub thread_id: String,
    pub highlight: Highlight,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
}

/// Detect trips by clustering travel highlights with overlapping date ranges.
///
/// Algorithm:
/// 1. Collect all Flight and Hotel highlights with parseable dates.
/// 2. Sort by start_date.
/// 3. Greedily merge items whose date ranges overlap or are adjacent (within 1 day).
/// 4. Each cluster with 2+ items becomes a TripBundle.
///
/// Returns a list of TripBundles, each containing the thread IDs and highlights
/// that form the trip.
pub fn detect_trips(items: Vec<TravelItem>) -> Vec<TripBundle> {
    if items.is_empty() {
        return Vec::new();
    }

    // Filter to items with at least a start date
    let mut dated: Vec<TravelItem> = items.into_iter()
        .filter(|i| i.start_date.is_some())
        .collect();

    if dated.is_empty() {
        return Vec::new();
    }

    dated.sort_by_key(|i| i.start_date.unwrap());

    // Greedy clustering
    let mut clusters: Vec<Vec<TravelItem>> = Vec::new();
    let mut current_cluster = vec![dated.remove(0)];
    let mut cluster_end = current_cluster[0].end_date
        .or(current_cluster[0].start_date)
        .unwrap();

    for item in dated {
        let item_start = item.start_date.unwrap();
        // Overlap or adjacent (within 1 day gap)
        if item_start <= cluster_end + Duration::days(1) {
            if let Some(end) = item.end_date.or(Some(item_start)) {
                if end > cluster_end {
                    cluster_end = end;
                }
            }
            current_cluster.push(item);
        } else {
            if current_cluster.len() >= 2 {
                clusters.push(std::mem::take(&mut current_cluster));
            } else {
                current_cluster.clear();
            }
            cluster_end = item.end_date.or(item.start_date).unwrap();
            current_cluster.push(item);
        }
    }
    if current_cluster.len() >= 2 {
        clusters.push(current_cluster);
    }

    // Convert clusters to TripBundles
    clusters.into_iter().map(|cluster| {
        let start = cluster.iter()
            .filter_map(|i| i.start_date)
            .min()
            .unwrap();
        let end = cluster.iter()
            .filter_map(|i| i.end_date.or(i.start_date))
            .max()
            .unwrap();

        let destination = infer_destination(&cluster);
        let thread_ids: Vec<String> = cluster.iter()
            .map(|i| i.thread_id.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let highlights: Vec<Highlight> = cluster.into_iter()
            .map(|i| i.highlight)
            .collect();

        TripBundle {
            id: uuid::Uuid::new_v4().to_string(),
            destination,
            start_date: start,
            end_date: end,
            thread_ids,
            highlights,
        }
    }).collect()
}

/// Infer destination label from flight arrival cities or hotel names.
fn infer_destination(items: &[TravelItem]) -> String {
    // Priority: flight arrival > hotel name > "Trip"
    for item in items {
        if let Highlight::Flight { arrive, .. } = &item.highlight {
            if let Some(arrive) = arrive {
                return arrive.clone();
            }
        }
    }
    for item in items {
        if let Highlight::Hotel { name, .. } = &item.highlight {
            return format!("Trip to {}", name);
        }
    }
    "Trip".to_string()
}
```

**Note:** The `TripBundle` type must be defined in `inboxly-core`. Verify M1 included it (the spec shows `TripBundle` in the data model at line 194). If not already defined:

```rust
// in inboxly-core/src/types.rs
pub struct TripBundle {
    pub id: String,
    pub destination: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub thread_ids: Vec<String>,
    pub highlights: Vec<Highlight>,
}
```

**Tests:**

1. Two flights on consecutive days → one trip
2. Flight + hotel with overlapping dates → one trip
3. Two flights 2 weeks apart → two separate items, no trip (need 2+ items)
4. Three items clustering → single trip with correct date range
5. Destination inferred from flight arrival
6. Destination falls back to hotel name
7. Empty input → empty output
8. Items with no parseable dates → excluded

**Commit:** `feat(extract): add trip detection algorithm with date range clustering`

---

### Task 13 — Date parsing utilities for travel highlights

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-extract/src/trips.rs` (extend) or `src/date_parse.rs` (new)

Parse date strings from flight departure/arrival and hotel check-in/checkout fields into `NaiveDate`. These fields contain free-form text extracted by regex, so parsing must be lenient.

```rust
use chrono::NaiveDate;
use once_cell::sync::Lazy;
use regex::Regex;

/// Common date formats found in email confirmations.
static DATE_FORMATS: &[&str] = &[
    "%Y-%m-%d",           // 2026-03-20
    "%B %d, %Y",          // March 20, 2026
    "%b %d, %Y",          // Mar 20, 2026
    "%d %B %Y",           // 20 March 2026
    "%d %b %Y",           // 20 Mar 2026
    "%m/%d/%Y",           // 03/20/2026
    "%d/%m/%Y",           // 20/03/2026
    "%Y%m%dT%H%M%S",     // iCal DTSTART format: 20260320T140000
    "%Y%m%dT%H%M%SZ",    // iCal UTC: 20260320T140000Z
    "%Y%m%d",             // iCal date-only: 20260320
];

/// Attempt to parse a date from a free-form string.
/// Tries multiple formats and returns the first successful parse.
pub fn parse_date_lenient(text: &str) -> Option<NaiveDate> {
    let trimmed = text.trim();
    for fmt in DATE_FORMATS {
        if let Ok(date) = NaiveDate::parse_from_str(trimmed, fmt) {
            return Some(date);
        }
        // Also try parsing as NaiveDateTime and extracting the date
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(trimmed, fmt) {
            return Some(dt.date());
        }
    }
    None
}

/// Convert a Highlight into a TravelItem with parsed dates.
pub fn highlight_to_travel_item(thread_id: &str, h: &Highlight) -> Option<TravelItem> {
    match h {
        Highlight::Flight { depart, arrive, .. } => {
            let start = depart.as_deref().and_then(parse_date_lenient);
            let end = arrive.as_deref().and_then(parse_date_lenient);
            Some(TravelItem {
                thread_id: thread_id.to_string(),
                highlight: h.clone(),
                start_date: start,
                end_date: end.or(start),
            })
        }
        Highlight::Hotel { checkin, checkout, .. } => {
            let start = checkin.as_deref().and_then(parse_date_lenient);
            let end = checkout.as_deref().and_then(parse_date_lenient);
            Some(TravelItem {
                thread_id: thread_id.to_string(),
                highlight: h.clone(),
                start_date: start,
                end_date: end,
            })
        }
        _ => None,
    }
}
```

**Tests:**

1. `"2026-03-20"` → `NaiveDate(2026, 3, 20)`
2. `"March 20, 2026"` → correct date
3. `"20260320T140000Z"` (iCal format) → correct date
4. `"garbage text"` → `None`
5. `highlight_to_travel_item` for Flight → TravelItem with dates
6. `highlight_to_travel_item` for Hotel → TravelItem with dates
7. `highlight_to_travel_item` for Payment → `None`

**Commit:** `feat(extract): add lenient date parsing for travel highlights`

---

### Task 14 — Trip bundle store and periodic sweep

**Files:**
- Create: `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/trips.rs`
- Modify: `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/lib.rs` (add `pub mod trips`)

**Schema** — add `trip_bundles` table (migration):

```sql
CREATE TABLE IF NOT EXISTS trip_bundles (
    id TEXT PRIMARY KEY,
    destination TEXT NOT NULL,
    start_date TEXT NOT NULL,    -- ISO 8601 date
    end_date TEXT NOT NULL,
    thread_ids_json TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);
```

**`trips.rs`:**

```rust
use rusqlite::{Connection, params};
use inboxly_core::TripBundle;
use inboxly_extract::trips::{detect_trips, highlight_to_travel_item};

/// Run trip detection over all travel highlights in the database.
/// Replaces any existing trip bundles (full rebuild each time).
///
/// Called periodically (e.g., after sync completes, or on a timer).
pub fn rebuild_trip_bundles(conn: &Connection) -> Result<usize, StoreError> {
    // 1. Fetch all flight and hotel highlights
    let flights = get_highlights_by_type(conn, "flight")?;
    let hotels = get_highlights_by_type(conn, "hotel")?;

    // 2. Convert to TravelItems
    let mut items = Vec::new();
    for (tid, h) in flights.iter().chain(hotels.iter()) {
        if let Some(item) = highlight_to_travel_item(tid, h) {
            items.push(item);
        }
    }

    // 3. Detect trips
    let trips = detect_trips(items);
    let count = trips.len();

    // 4. Replace existing trip bundles
    conn.execute("DELETE FROM trip_bundles", [])?;
    for trip in &trips {
        conn.execute(
            "INSERT INTO trip_bundles (id, destination, start_date, end_date, thread_ids_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                trip.id,
                trip.destination,
                trip.start_date.to_string(),
                trip.end_date.to_string(),
                serde_json::to_string(&trip.thread_ids).unwrap(),
            ],
        )?;
    }

    Ok(count)
}

/// Get all trip bundles, ordered by start date.
pub fn get_trip_bundles(conn: &Connection) -> Result<Vec<TripBundle>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT id, destination, start_date, end_date, thread_ids_json
         FROM trip_bundles ORDER BY start_date ASC"
    )?;
    let rows = stmt.query_map([], |row| {
        let id: String = row.get(0)?;
        let destination: String = row.get(1)?;
        let start: String = row.get(2)?;
        let end: String = row.get(3)?;
        let tids_json: String = row.get(4)?;
        Ok((id, destination, start, end, tids_json))
    })?;
    let mut bundles = Vec::new();
    for row in rows {
        let (id, destination, start_str, end_str, tids_json) = row?;
        let start_date = chrono::NaiveDate::parse_from_str(&start_str, "%Y-%m-%d")
            .map_err(|e| StoreError::Internal(e.to_string()))?;
        let end_date = chrono::NaiveDate::parse_from_str(&end_str, "%Y-%m-%d")
            .map_err(|e| StoreError::Internal(e.to_string()))?;
        let thread_ids: Vec<String> = serde_json::from_str(&tids_json)
            .unwrap_or_default();

        // Load highlights for these threads
        let mut highlights = Vec::new();
        for tid in &thread_ids {
            let mut ths = get_highlights_for_thread(conn, tid)?;
            highlights.append(&mut ths);
        }

        bundles.push(TripBundle {
            id,
            destination,
            start_date,
            end_date,
            thread_ids,
            highlights,
        });
    }
    Ok(bundles)
}

/// Get a single trip bundle by ID.
pub fn get_trip_bundle(conn: &Connection, id: &str) -> Result<Option<TripBundle>, StoreError>;
```

**Tests:**

1. Insert flight + hotel highlights → rebuild finds 1 trip
2. No travel highlights → no trips
3. Round-trip: rebuild → get_trip_bundles → correct data
4. Rebuild is idempotent (run twice, same result)

**Commit:** `feat(store): add trip bundle detection, storage, and query API`

---

## D. Multi-Account

### Task 15 — Account CRUD in store

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-store/src/accounts.rs` (new or extend existing)

The `accounts` table should exist from M3. Add full CRUD operations.

```rust
use rusqlite::{Connection, params};
use inboxly_core::{AccountId, Account};

/// Insert a new account. Returns the generated AccountId.
pub fn add_account(conn: &Connection, account: &Account) -> Result<AccountId, StoreError> {
    conn.execute(
        "INSERT INTO accounts (id, email, display_name, provider, auth_method,
         imap_host, imap_port, smtp_host, smtp_port)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            account.id.to_string(),
            account.email,
            account.display_name,
            account.provider,
            account.auth_method.to_string(),
            account.imap_host,
            account.imap_port,
            account.smtp_host,
            account.smtp_port,
        ],
    )?;
    Ok(account.id.clone())
}

/// Remove an account and all associated data (emails, threads, highlights, sync state).
/// Uses CASCADE deletes where available, explicit cleanup otherwise.
pub fn remove_account(conn: &Connection, account_id: &AccountId) -> Result<(), StoreError> {
    let tx = conn.transaction()?;
    let id_str = account_id.to_string();

    // Delete in dependency order
    tx.execute("DELETE FROM highlights WHERE thread_id IN (SELECT id FROM threads WHERE account_id = ?1)", params![id_str])?;
    tx.execute("DELETE FROM thread_state WHERE thread_id IN (SELECT id FROM threads WHERE account_id = ?1)", params![id_str])?;
    tx.execute("DELETE FROM emails WHERE account_id = ?1", params![id_str])?;
    tx.execute("DELETE FROM threads WHERE account_id = ?1", params![id_str])?;
    tx.execute("DELETE FROM sync_state WHERE account_id = ?1", params![id_str])?;
    tx.execute("DELETE FROM sender_affinity WHERE sender_address IN (SELECT DISTINCT from_address FROM emails WHERE account_id = ?1)", params![id_str])?;
    tx.execute("DELETE FROM trip_bundles WHERE thread_ids_json LIKE ?1", params![format!("%{}%", id_str)])?;
    tx.execute("DELETE FROM accounts WHERE id = ?1", params![id_str])?;

    tx.commit()?;

    // Also clean up Maildir directory for this account (caller responsibility —
    // store returns the path, caller deletes the directory)
    Ok(())
}

/// List all accounts.
pub fn list_accounts(conn: &Connection) -> Result<Vec<Account>, StoreError>;

/// Get a single account by ID.
pub fn get_account(conn: &Connection, id: &AccountId) -> Result<Option<Account>, StoreError>;

/// Update account settings (display name, server config).
pub fn update_account(conn: &Connection, account: &Account) -> Result<(), StoreError>;
```

**Tests:**

1. Add account → list_accounts returns it
2. Remove account → all associated data deleted
3. Update account → changes persisted
4. Get non-existent account → None

**Commit:** `feat(store): add account CRUD operations`

---

### Task 16 — Per-account sync task spawning

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-imap/src/sync_manager.rs` (new or extend)

Manage independent sync tasks for each account.

```rust
use std::collections::HashMap;
use tokio::task::JoinHandle;
use tokio::sync::mpsc;
use inboxly_core::AccountId;

/// Manages per-account sync tasks.
pub struct SyncManager {
    tasks: HashMap<String, SyncTask>,
    event_tx: mpsc::Sender<SyncEvent>,
}

struct SyncTask {
    account_id: String,
    handle: JoinHandle<()>,
    cancel: tokio::sync::watch::Sender<bool>,
}

impl SyncManager {
    pub fn new(event_tx: mpsc::Sender<SyncEvent>) -> Self {
        Self {
            tasks: HashMap::new(),
            event_tx,
        }
    }

    /// Start sync for an account. If already running, no-op.
    pub async fn start_account_sync(
        &mut self,
        account_id: &str,
        config: AccountSyncConfig,
    ) {
        if self.tasks.contains_key(account_id) {
            return;
        }

        let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        let event_tx = self.event_tx.clone();
        let aid = account_id.to_string();

        let handle = tokio::spawn(async move {
            run_account_sync(aid, config, event_tx, cancel_rx).await;
        });

        self.tasks.insert(account_id.to_string(), SyncTask {
            account_id: account_id.to_string(),
            handle,
            cancel: cancel_tx,
        });
    }

    /// Stop sync for an account. Cancels IDLE and disconnects.
    pub async fn stop_account_sync(&mut self, account_id: &str) {
        if let Some(task) = self.tasks.remove(account_id) {
            let _ = task.cancel.send(true);
            let _ = task.handle.await;
        }
    }

    /// Stop all syncs (used during shutdown).
    pub async fn stop_all(&mut self) {
        let ids: Vec<String> = self.tasks.keys().cloned().collect();
        for id in ids {
            self.stop_account_sync(&id).await;
        }
    }

    /// Check which accounts are currently syncing.
    pub fn active_accounts(&self) -> Vec<String> {
        self.tasks.keys().cloned().collect()
    }
}

/// Run the full sync lifecycle for a single account.
/// This is the per-account task entry point.
async fn run_account_sync(
    account_id: String,
    config: AccountSyncConfig,
    event_tx: mpsc::Sender<SyncEvent>,
    mut cancel_rx: tokio::sync::watch::Receiver<bool>,
) {
    // 1. Connect (IMAP)
    // 2. Initial or incremental sync
    // 3. Enter IDLE loop
    // 4. On cancel signal, disconnect and return
    //
    // Uses existing sync logic from M7-M9, parameterized by account config.
    // Each account has its own IMAP connection — one slow/broken account
    // doesn't block others (per spec §Design Decisions).
}
```

**Integration with binary:** Modify the app startup in `inboxly/src/main.rs` to:
1. Load all accounts from SQLite
2. Start sync for each account via `SyncManager`
3. When user adds/removes an account, call start/stop on `SyncManager`

**Tests:**

1. Start sync for 2 accounts → both running
2. Stop one account → only that one stops
3. Stop all → clean shutdown
4. Start sync for already-syncing account → no-op

**Commit:** `feat(imap): add SyncManager for per-account independent sync tasks`

---

### Task 17 — Account add/remove settings UI

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/settings.rs` (modify or extend)

Add an account management section to the settings view.

**UI structure:**

```
┌─ Accounts ─────────────────────────────────────┐
│                                                 │
│  ┌──────────────────────────────────────┐       │
│  │ 📧 alan@example.com                  │ [✕]  │
│  │    IMAP: imap.example.com:993        │       │
│  └──────────────────────────────────────┘       │
│                                                 │
│  ┌──────────────────────────────────────┐       │
│  │ 📧 work@company.com                  │ [✕]  │
│  │    IMAP: imap.company.com:993        │       │
│  └──────────────────────────────────────┘       │
│                                                 │
│  [+ Add Account]                                │
│                                                 │
└─────────────────────────────────────────────────┘
```

**Add Account flow** — modal dialog with fields:
- Email address (text input)
- Display name (text input)
- Provider preset dropdown (Gmail, Fastmail, Custom)
  - Gmail: auto-fills IMAP/SMTP hosts, sets OAuth2 auth
  - Fastmail: auto-fills hosts, sets AppPassword auth
  - Custom: all fields editable
- IMAP host + port (text inputs, auto-filled by provider)
- SMTP host + port (text inputs, auto-filled by provider)
- Auth method (dropdown: Password, OAuth2, App Password)
- Password / App Password (text input, hidden)
- [Test Connection] button → attempts IMAP LOGIN, shows success/failure
- [Save] button → calls `add_account()`, starts sync via `SyncManager`

**Remove Account flow:**
- Click [x] on account row
- Confirmation dialog: "Remove account alan@example.com? This will delete all local emails for this account."
- On confirm: calls `remove_account()`, stops sync, deletes Maildir directory

**Messages:**

```rust
pub enum SettingsMessage {
    // ... existing messages ...
    AddAccountPressed,
    AddAccountFieldChanged(AddAccountField, String),
    ProviderSelected(ProviderPreset),
    TestConnectionPressed,
    TestConnectionResult(Result<(), String>),
    SaveAccountPressed,
    RemoveAccountPressed(AccountId),
    RemoveAccountConfirmed(AccountId),
    RemoveAccountCancelled,
}
```

**Commit:** `feat(ui): add account add/remove settings UI`

---

### Task 18 — Account switcher in nav drawer

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/nav_drawer.rs` (modify)

Add an account switcher at the top of the nav drawer.

**Layout:**

```
┌─────────────────────┐
│  [avatar] Alan       │  ← current account, clickable
│  alan@example.com   ▼│  ← dropdown arrow
├─────────────────────┤
│ Inbox          ● 3   │
│ Snoozed              │
│ Done                 │
│ ...                  │
```

Clicking the account area shows a dropdown overlay listing all accounts:

```
┌─────────────────────┐
│ ✓ alan@example.com   │  ← current (checkmark)
│   work@company.com   │
│ ─────────────────── │
│   All Inboxes        │  ← unified inbox toggle
│ ─────────────────── │
│   Add account        │  → opens settings
│   Manage accounts    │  → opens settings
└─────────────────────┘
```

**State:**

```rust
pub struct NavDrawerState {
    // ... existing fields ...
    current_account: Option<AccountId>,
    account_dropdown_open: bool,
    accounts: Vec<Account>,
}
```

**Messages:**

```rust
pub enum NavMessage {
    // ... existing messages ...
    AccountDropdownToggled,
    AccountSelected(AccountId),
    UnifiedInboxSelected,
    ManageAccountsPressed,
}
```

When an account is selected:
1. Update `current_account` in state
2. Filter inbox feed to show only that account's emails/threads
3. Update toolbar to show account name

When "All Inboxes" is selected:
1. Set `current_account` to None
2. Show merged feed from all accounts

**Commit:** `feat(ui): add account switcher dropdown to nav drawer`

---

### Task 19 — Unified inbox feed

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/inbox.rs` (modify)

When unified inbox is active (`current_account == None`), the feed merges threads from all accounts.

**Changes:**

```rust
/// Build the inbox feed items.
fn build_feed(&self) -> Vec<InboxItem> {
    let threads = match &self.current_account {
        Some(account_id) => {
            // Single account: filter threads by account_id
            self.store.get_active_threads(Some(account_id))
        }
        None => {
            // Unified inbox: all accounts merged
            self.store.get_active_threads(None)
        }
    };

    // Sort merged threads by newest_date descending
    // Bundle assignment still works per-thread
    // Trip bundles may span multiple accounts (same destination)
    // ...existing feed building logic...
}
```

In unified mode, add a small account indicator (colored dot or email abbreviation) on each email row so the user knows which account it belongs to.

**Commit:** `feat(ui): implement unified inbox feed for multi-account`

---

## E. Inbox Zero Sun

### Task 20 — InboxZeroSun canvas widget

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/inbox_zero_sun.rs` (new)

Custom Iced `Canvas` widget that draws the iconic Google Inbox sun illustration.

**Design:**
- Centered in the inbox feed area when there are zero active items
- Sun circle: warm yellow/orange gradient
- Rays: alternating triangular rays around the circle, subtle rotation animation
- Face: two dot eyes and a gentle smile arc
- Below: "You're all done!" text in secondary text color
- Optional: gentle pulsing/breathing animation on the sun (scale oscillation ±2%)

```rust
use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path, Program, Stroke};
use iced::{Color, Point, Size, Rectangle, Theme, Renderer, mouse};

pub struct InboxZeroSun {
    animation_t: f32,  // 0.0 to 1.0, loops for breathing animation
}

impl InboxZeroSun {
    pub fn new() -> Self {
        Self { animation_t: 0.0 }
    }

    pub fn tick(&mut self, dt: f32) {
        self.animation_t = (self.animation_t + dt * 0.5) % 1.0;
    }
}

impl<Message> Program<Message> for InboxZeroSun {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let center = Point::new(bounds.width / 2.0, bounds.height / 2.0 - 40.0);

        // Breathing scale
        let scale = 1.0 + 0.02 * (self.animation_t * std::f32::consts::TAU).sin();
        let radius = 60.0 * scale;

        // Draw rays (12 triangular rays)
        let ray_count = 12;
        let ray_length = radius * 1.6;
        let ray_width = std::f32::consts::PI / (ray_count as f32);
        let ray_color = Color::from_rgb(1.0, 0.82, 0.28); // warm yellow

        for i in 0..ray_count {
            let angle = (i as f32) * 2.0 * std::f32::consts::PI / (ray_count as f32);
            let tip = Point::new(
                center.x + ray_length * angle.cos(),
                center.y + ray_length * angle.sin(),
            );
            let left = Point::new(
                center.x + radius * 0.9 * (angle - ray_width * 0.4).cos(),
                center.y + radius * 0.9 * (angle - ray_width * 0.4).sin(),
            );
            let right = Point::new(
                center.x + radius * 0.9 * (angle + ray_width * 0.4).cos(),
                center.y + radius * 0.9 * (angle + ray_width * 0.4).sin(),
            );

            let ray = Path::new(|b| {
                b.move_to(left);
                b.line_to(tip);
                b.line_to(right);
                b.close();
            });
            frame.fill(&ray, ray_color);
        }

        // Draw sun circle
        let sun_color = Color::from_rgb(1.0, 0.76, 0.03); // #FFC207-ish
        let sun = Path::circle(center, radius);
        frame.fill(&sun, sun_color);

        // Draw eyes (two dots)
        let eye_y = center.y - radius * 0.15;
        let eye_offset = radius * 0.25;
        let eye_radius = radius * 0.06;
        let eye_color = Color::from_rgb(0.4, 0.3, 0.1);
        frame.fill(&Path::circle(Point::new(center.x - eye_offset, eye_y), eye_radius), eye_color);
        frame.fill(&Path::circle(Point::new(center.x + eye_offset, eye_y), eye_radius), eye_color);

        // Draw smile (arc)
        let smile = Path::new(|b| {
            let smile_y = center.y + radius * 0.1;
            let smile_width = radius * 0.35;
            b.move_to(Point::new(center.x - smile_width, smile_y));
            b.quadratic_curve_to(
                Point::new(center.x, smile_y + radius * 0.2),
                Point::new(center.x + smile_width, smile_y),
            );
        });
        frame.stroke(
            &smile,
            Stroke::default()
                .with_color(eye_color)
                .with_width(2.0),
        );

        vec![frame.into_geometry()]
    }
}
```

**Widget wrapper** that includes the "You're all done!" text below the canvas:

```rust
pub fn inbox_zero_view<'a>() -> iced::Element<'a, Message> {
    let sun_canvas = Canvas::new(InboxZeroSun::new())
        .width(200)
        .height(200);

    let text = iced::widget::text("You're all done!")
        .size(20)
        .color(Color::from_rgb(0.46, 0.46, 0.46)); // secondary text

    let subtitle = iced::widget::text("Enjoy your day.")
        .size(14)
        .color(Color::from_rgb(0.62, 0.62, 0.62));

    iced::widget::column![sun_canvas, text, subtitle]
        .align_x(iced::Alignment::Center)
        .spacing(8)
        .padding(40)
        .into()
}
```

**Tests:**

1. Widget renders without panic (smoke test)
2. `tick()` advances animation_t correctly
3. Animation_t wraps at 1.0

**Commit:** `feat(ui): add Inbox Zero Sun canvas widget`

---

### Task 21 — Integrate Inbox Zero Sun into inbox feed

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/inbox.rs` (modify)

Show the sun when the inbox feed is empty (no active threads, no pinned items, no pending reminders).

```rust
fn view_inbox_feed(&self) -> Element<Message> {
    let items = self.build_feed();

    if items.is_empty() {
        return inbox_zero_view();
    }

    // ... existing feed rendering ...
}
```

Add a subscription for the breathing animation tick:

```rust
fn subscription(&self) -> Subscription<Message> {
    let mut subs = vec![/* ...existing subs... */];

    if self.feed_is_empty {
        subs.push(
            iced::time::every(std::time::Duration::from_millis(16))
                .map(|_| Message::InboxZeroTick)
        );
    }

    Subscription::batch(subs)
}
```

**Commit:** `feat(ui): show Inbox Zero Sun when inbox feed is empty`

---

## F. Animations

### Task 22 — Animation utilities module

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/animation.rs` (new)

Shared animation primitives used by all animated widgets.

```rust
use std::time::{Duration, Instant};

/// Easing functions.
pub mod easing {
    /// Ease-out cubic: decelerates towards end.
    pub fn ease_out_cubic(t: f32) -> f32 {
        let t = t - 1.0;
        t * t * t + 1.0
    }

    /// Ease-in-out cubic: smooth start and end.
    pub fn ease_in_out_cubic(t: f32) -> f32 {
        if t < 0.5 {
            4.0 * t * t * t
        } else {
            let t = -2.0 * t + 2.0;
            1.0 - t * t * t / 2.0
        }
    }

    /// Linear (identity).
    pub fn linear(t: f32) -> f32 { t }
}

/// A single animation timeline.
#[derive(Debug, Clone)]
pub struct Animation {
    start: Instant,
    duration: Duration,
    direction: AnimationDirection,
}

#[derive(Debug, Clone, Copy)]
pub enum AnimationDirection {
    Forward,
    Reverse,
}

impl Animation {
    /// Create a new animation starting now.
    pub fn start(duration: Duration) -> Self {
        Self {
            start: Instant::now(),
            duration,
            direction: AnimationDirection::Forward,
        }
    }

    /// Create a reverse animation.
    pub fn start_reverse(duration: Duration) -> Self {
        Self {
            start: Instant::now(),
            duration,
            direction: AnimationDirection::Reverse,
        }
    }

    /// Get the raw progress (0.0 to 1.0), clamped.
    pub fn progress(&self) -> f32 {
        let elapsed = self.start.elapsed().as_secs_f32();
        let total = self.duration.as_secs_f32();
        let p = (elapsed / total).clamp(0.0, 1.0);
        match self.direction {
            AnimationDirection::Forward => p,
            AnimationDirection::Reverse => 1.0 - p,
        }
    }

    /// Get eased progress using the given easing function.
    pub fn eased(&self, ease_fn: fn(f32) -> f32) -> f32 {
        ease_fn(self.progress())
    }

    /// Is the animation complete?
    pub fn is_complete(&self) -> bool {
        self.start.elapsed() >= self.duration
    }
}

/// Staggered animation helper — calculates offset for item N in a cascade.
pub fn stagger_delay(index: usize, per_item: Duration) -> Duration {
    Duration::from_millis(index as u64 * per_item.as_millis() as u64)
}

/// Create a staggered animation that starts after a delay.
pub fn staggered_animation(index: usize, per_item: Duration, item_duration: Duration) -> Animation {
    let delay = stagger_delay(index, per_item);
    Animation {
        start: Instant::now() + delay,
        duration: item_duration,
        direction: AnimationDirection::Forward,
    }
}
```

**Tests:**

1. Animation progress at t=0 is 0.0
2. Animation progress after full duration is 1.0
3. ease_out_cubic(0.0) = 0.0, ease_out_cubic(1.0) = 1.0
4. Reverse animation: progress at t=0 is 1.0
5. stagger_delay for index 3 with 50ms per item = 150ms

**Commit:** `feat(ui): add animation utilities with easing functions`

---

### Task 23 — Bundle expand/collapse animation

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/bundle_row.rs` (modify)

Animate the in-place expansion of bundle rows. When a bundle is clicked:
- Container height transitions from collapsed (single row ~72dp) to expanded (row + all children)
- Items above slide up, items below slide down (handled by layout engine reacting to height change)
- Duration: 250-300ms, ease-out

**Implementation approach:**

```rust
use crate::animation::{Animation, easing};
use std::time::Duration;

pub struct BundleRowState {
    // ... existing fields ...
    expand_animation: Option<Animation>,
    is_expanded: bool,
    /// Height multiplier: 0.0 = collapsed, 1.0 = fully expanded.
    expand_progress: f32,
}

impl BundleRowState {
    pub fn toggle_expand(&mut self) {
        let duration = Duration::from_millis(275);
        if self.is_expanded {
            self.expand_animation = Some(Animation::start_reverse(duration));
            self.is_expanded = false;
        } else {
            self.expand_animation = Some(Animation::start(duration));
            self.is_expanded = true;
        }
    }

    pub fn tick(&mut self) {
        if let Some(ref anim) = self.expand_animation {
            self.expand_progress = anim.eased(easing::ease_out_cubic);
            if anim.is_complete() {
                self.expand_progress = if self.is_expanded { 1.0 } else { 0.0 };
                self.expand_animation = None;
            }
        }
    }
}
```

In the `view` method, use `expand_progress` to interpolate the height of the children container:

```rust
fn view_bundle(&self) -> Element<Message> {
    let header = self.view_bundle_header();

    if self.expand_progress <= 0.0 {
        return header;
    }

    let children_height = self.children_full_height * self.expand_progress;
    let children = iced::widget::container(self.view_bundle_children())
        .height(children_height)
        .clip(true); // clip overflow during animation

    iced::widget::column![header, children].into()
}
```

**Commit:** `feat(ui): animate bundle expand/collapse with 275ms ease-out`

---

### Task 24 — Sweep cascade animation

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/inbox.rs` (modify sweep handling)

When sweep is triggered, rows collapse upward with ~50ms stagger per row.

```rust
use crate::animation::{staggered_animation, easing};
use std::time::Duration;

pub struct SweepState {
    /// Per-row animation state. Key = thread_id, value = animation.
    row_animations: HashMap<String, Animation>,
    /// IDs being swept, in visual order.
    sweeping_ids: Vec<String>,
}

impl SweepState {
    /// Start the sweep cascade animation.
    pub fn start_sweep(&mut self, thread_ids: Vec<String>) {
        self.sweeping_ids = thread_ids;
        self.row_animations.clear();

        let per_item = Duration::from_millis(50);
        let item_duration = Duration::from_millis(200);

        for (i, id) in self.sweeping_ids.iter().enumerate() {
            self.row_animations.insert(
                id.clone(),
                staggered_animation(i, per_item, item_duration),
            );
        }
    }

    /// Get the collapse progress for a row (1.0 = visible, 0.0 = collapsed).
    pub fn row_progress(&self, thread_id: &str) -> f32 {
        self.row_animations.get(thread_id)
            .map(|anim| 1.0 - anim.eased(easing::ease_out_cubic))
            .unwrap_or(1.0)
    }

    /// Are all sweep animations complete?
    pub fn is_complete(&self) -> bool {
        self.row_animations.values().all(|a| a.is_complete())
    }
}
```

In the feed view, during active sweep, scale each row's height by its progress:

```rust
fn view_feed_row(&self, item: &InboxItem) -> Element<Message> {
    let row = self.render_row(item);

    if let Some(progress) = self.sweep_state.as_ref()
        .map(|s| s.row_progress(item.id()))
    {
        if progress <= 0.01 {
            return iced::widget::Space::new(0, 0).into(); // fully collapsed
        }
        let height = ROW_HEIGHT * progress;
        return iced::widget::container(row)
            .height(height)
            .clip(true)
            .into();
    }

    row
}
```

After all animations complete, actually apply the Done state to all swept threads and show the undo snackbar.

**Commit:** `feat(ui): add sweep cascade animation with 50ms stagger`

---

### Task 25 — FAB speed dial animation

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/speed_dial_fab.rs` (modify)

Animate the FAB opening: main button icon rotates (+ → x), options fly in from below with staggered fade+slide.

```rust
pub struct SpeedDialState {
    open_animation: Option<Animation>,
    is_open: bool,
    /// 0.0 = closed, 1.0 = open
    progress: f32,
}

impl SpeedDialState {
    pub fn toggle(&mut self) {
        let duration = Duration::from_millis(250);
        if self.is_open {
            self.open_animation = Some(Animation::start_reverse(duration));
            self.is_open = false;
        } else {
            self.open_animation = Some(Animation::start(duration));
            self.is_open = true;
        }
    }

    pub fn tick(&mut self) {
        if let Some(ref anim) = self.open_animation {
            self.progress = anim.eased(easing::ease_out_cubic);
            if anim.is_complete() {
                self.progress = if self.is_open { 1.0 } else { 0.0 };
                self.open_animation = None;
            }
        }
    }
}
```

In the view:

```rust
fn view_fab(&self) -> Element<Message> {
    let rotation = self.speed_dial.progress * 45.0; // rotate + icon 45 degrees

    let main_button = /* ... FAB with rotated icon ... */;

    let mut items = vec![main_button];

    if self.speed_dial.progress > 0.0 {
        // Options fly in with stagger
        let options = [("Compose", FabAction::Compose), ("Reminder", FabAction::Reminder)];
        for (i, (label, action)) in options.iter().enumerate() {
            let item_progress = (self.speed_dial.progress - (i as f32 * 0.15)).clamp(0.0, 1.0);
            let opacity = item_progress;
            let offset_y = (1.0 - item_progress) * 20.0; // slide up 20dp

            let option_row = iced::widget::row![
                iced::widget::text(label).size(14),
                /* mini FAB icon */
            ]
            .spacing(8);

            // Apply opacity and vertical offset via container transform
            let animated = iced::widget::container(option_row)
                .style(move |_| {
                    iced::widget::container::Style {
                        // Use opacity via color alpha
                        ..Default::default()
                    }
                });

            items.insert(0, animated.into()); // prepend (above main FAB)
        }
    }

    // Scrim overlay when open
    if self.speed_dial.progress > 0.1 {
        // Semi-transparent overlay behind speed dial
    }

    iced::widget::column(items)
        .spacing(12)
        .align_x(iced::Alignment::End)
        .into()
}
```

**Commit:** `feat(ui): animate FAB speed dial with rotate and staggered fly-in`

---

### Task 26 — Toolbar colour crossfade on view switch

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/toolbar.rs` (modify)

When switching between Inbox (blue), Snoozed (orange), and Done (green) views, crossfade the toolbar background colour over 200ms.

```rust
pub struct ToolbarState {
    current_color: Color,
    target_color: Color,
    color_animation: Option<Animation>,
}

impl ToolbarState {
    pub fn set_view(&mut self, view: View) {
        let new_color = match view {
            View::Inbox => Color::from_rgb(0.259, 0.522, 0.957),    // #4285f4
            View::Snoozed => Color::from_rgb(0.937, 0.424, 0.0),    // #ef6c00
            View::Done => Color::from_rgb(0.059, 0.616, 0.345),     // #0f9d58
        };

        if new_color != self.target_color {
            self.current_color = self.interpolated_color(); // capture current mid-transition color
            self.target_color = new_color;
            self.color_animation = Some(Animation::start(Duration::from_millis(200)));
        }
    }

    pub fn tick(&mut self) {
        if let Some(ref anim) = self.color_animation {
            if anim.is_complete() {
                self.current_color = self.target_color;
                self.color_animation = None;
            }
        }
    }

    /// Get the current interpolated toolbar color.
    pub fn interpolated_color(&self) -> Color {
        match &self.color_animation {
            Some(anim) => {
                let t = anim.eased(easing::ease_in_out_cubic);
                lerp_color(self.current_color, self.target_color, t)
            }
            None => self.current_color,
        }
    }
}

/// Linear interpolation between two colors.
fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    Color::from_rgba(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}
```

**Commit:** `feat(ui): animate toolbar colour crossfade on view switch`

---

### Task 27 — Swipe commit animation

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/swipe_container.rs` (modify)

After swipe passes the commit threshold (50% of row width), animate:
1. Row slides off-screen in the swipe direction (100ms)
2. Gap collapses upward (200ms)

```rust
pub struct SwipeState {
    // ... existing drag state ...
    commit_animation: Option<SwipeCommitAnimation>,
}

struct SwipeCommitAnimation {
    /// Phase 1: row slides off (0.0 → 1.0 over 100ms)
    slide_off: Animation,
    /// Phase 2: gap collapses (0.0 → 1.0 over 200ms, starts after slide_off)
    collapse: Option<Animation>,
    direction: SwipeDirection,
}

impl SwipeState {
    pub fn commit_swipe(&mut self, direction: SwipeDirection) {
        self.commit_animation = Some(SwipeCommitAnimation {
            slide_off: Animation::start(Duration::from_millis(100)),
            collapse: None,
            direction,
        });
    }

    pub fn tick(&mut self) -> Option<SwipeCommitAction> {
        if let Some(ref mut commit) = self.commit_animation {
            if commit.slide_off.is_complete() && commit.collapse.is_none() {
                commit.collapse = Some(Animation::start(Duration::from_millis(200)));
            }
            if let Some(ref collapse) = commit.collapse {
                if collapse.is_complete() {
                    let action = match commit.direction {
                        SwipeDirection::Right => SwipeCommitAction::Done,
                        SwipeDirection::Left => SwipeCommitAction::Snooze,
                    };
                    self.commit_animation = None;
                    return Some(action);
                }
            }
        }
        None
    }

    /// Get the horizontal offset for the sliding row.
    pub fn slide_offset(&self, row_width: f32) -> f32 {
        if let Some(ref commit) = self.commit_animation {
            let progress = commit.slide_off.eased(easing::ease_out_cubic);
            match commit.direction {
                SwipeDirection::Right => row_width * progress,
                SwipeDirection::Left => -row_width * progress,
            }
        } else {
            0.0
        }
    }

    /// Get the height multiplier for the collapsing gap.
    pub fn collapse_progress(&self) -> f32 {
        if let Some(ref commit) = self.commit_animation {
            if let Some(ref collapse) = commit.collapse {
                return 1.0 - collapse.eased(easing::ease_out_cubic);
            }
        }
        1.0
    }
}
```

**Commit:** `feat(ui): animate swipe commit with slide-off and gap collapse`

---

## G. Highlight + Trip UI

### Task 28 — Highlight card widget

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/highlight_card.rs` (new)

Inline cards that appear below an email row when the thread has highlights.

**Layout per highlight type:**

```
┌─ Tracking ──────────────────────────────────────┐
│ 📦 UPS  1Z999AA10123456784           [Track →]  │
└─────────────────────────────────────────────────┘

┌─ Flight ────────────────────────────────────────┐
│ ✈️  AC 123                                       │
│ YUL → YVR  ·  Gate B42                          │
└─────────────────────────────────────────────────┘

┌─ Hotel ─────────────────────────────────────────┐
│ 🏨 Grand Hyatt   Conf: ABC12345                 │
│ Mar 20 → Mar 23                                 │
└─────────────────────────────────────────────────┘

┌─ Event ─────────────────────────────────────────┐
│ 📅 Team Standup                                  │
│ Mar 21, 2:00 PM  ·  Conference Room A           │
└─────────────────────────────────────────────────┘

┌─ Payment ───────────────────────────────────────┐
│ 💳 $123.45 USD  from PayPal                     │
└─────────────────────────────────────────────────┘
```

```rust
use iced::widget::{container, row, column, text, button};
use iced::{Element, Color, Length};
use inboxly_core::Highlight;

/// Render a highlight card for display below an email row.
pub fn highlight_card<'a>(highlight: &Highlight) -> Element<'a, Message> {
    match highlight {
        Highlight::TrackingNumber { carrier, number, url } => {
            let icon = text("📦").size(16);
            let label = text(format!("{}  {}", carrier, number)).size(14);
            let track_btn = button(text("Track →").size(12))
                .on_press(Message::OpenUrl(url.clone()))
                .style(link_button_style);

            card_container(
                row![icon, label, iced::widget::Space::with_width(Length::Fill), track_btn]
                    .spacing(8)
                    .align_y(iced::Alignment::Center)
            )
        }
        Highlight::Flight { airline, number, depart, arrive, gate } => {
            let icon = text("✈").size(16);
            let title = text(number).size(14).style(|_| text::Style { color: Some(Color::from_rgb(0.13, 0.13, 0.13)) });

            let mut detail_parts = Vec::new();
            if let (Some(d), Some(a)) = (depart, arrive) {
                detail_parts.push(format!("{} → {}", d, a));
            }
            if let Some(g) = gate {
                detail_parts.push(format!("Gate {}", g));
            }
            let detail = text(detail_parts.join("  ·  ")).size(12)
                .color(Color::from_rgb(0.46, 0.46, 0.46));

            card_container(
                column![
                    row![icon, title].spacing(8),
                    detail,
                ].spacing(4)
            )
        }
        Highlight::Hotel { name, checkin, checkout, confirmation } => {
            let icon = text("🏨").size(16);
            let title = text(name).size(14);
            let conf = text(format!("Conf: {}", confirmation)).size(12)
                .color(Color::from_rgb(0.46, 0.46, 0.46));

            let mut dates = String::new();
            if let Some(ci) = checkin {
                dates.push_str(ci);
            }
            if let Some(co) = checkout {
                dates.push_str(&format!(" → {}", co));
            }
            let date_text = text(dates).size(12)
                .color(Color::from_rgb(0.46, 0.46, 0.46));

            card_container(
                column![
                    row![icon, title, iced::widget::Space::with_width(Length::Fill), conf].spacing(8),
                    date_text,
                ].spacing(4)
            )
        }
        Highlight::Event { title: etitle, datetime, location } => {
            let icon = text("📅").size(16);
            let title = text(etitle).size(14);

            let mut detail_parts = Vec::new();
            if let Some(dt) = datetime {
                detail_parts.push(dt.clone());
            }
            if let Some(loc) = location {
                detail_parts.push(loc.clone());
            }
            let detail = text(detail_parts.join("  ·  ")).size(12)
                .color(Color::from_rgb(0.46, 0.46, 0.46));

            card_container(
                column![
                    row![icon, title].spacing(8),
                    detail,
                ].spacing(4)
            )
        }
        Highlight::Payment { amount, currency, from_or_to } => {
            let icon = text("💳").size(16);
            let label = text(format!("{} {}", amount, currency)).size(14);
            let party = from_or_to.as_ref()
                .map(|p| text(format!("from {}", p)).size(12).color(Color::from_rgb(0.46, 0.46, 0.46)))
                .unwrap_or_else(|| text("").size(12));

            card_container(
                row![icon, label, party].spacing(8).align_y(iced::Alignment::Center)
            )
        }
    }
}

/// Wrap content in a styled card container.
fn card_container<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .padding([8, 12])
        .style(|_| container::Style {
            background: Some(iced::Background::Color(Color::from_rgb(0.96, 0.97, 0.98))),
            border: iced::Border {
                radius: 4.0.into(),
                width: 1.0,
                color: Color::from_rgb(0.88, 0.88, 0.88),
            },
            ..Default::default()
        })
        .width(Length::Fill)
        .into()
}
```

**Commit:** `feat(ui): add highlight card widgets for all 5 highlight types`

---

### Task 29 — Trip bundle card widget

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/trip_card.rs` (new)

A card that represents a grouped trip in the inbox feed.

**Layout:**

```
┌─ Trip ──────────────────────────────────────────┐
│ ✈ Vancouver  ·  Mar 20 – Mar 25                │
│                                                 │
│  📦 Flight AC 123  YUL → YVR                    │
│  🏨 Fairmont Hotel  Conf: HX82931               │
│  📅 Conference Day 1  Mar 22                     │
│                                                 │
│  3 emails from 2 threads                        │
└─────────────────────────────────────────────────┘
```

```rust
use iced::widget::{container, column, row, text};
use iced::{Element, Color, Length};
use inboxly_core::TripBundle;

pub fn trip_bundle_card<'a>(trip: &TripBundle) -> Element<'a, Message> {
    let header_icon = text("✈").size(18);
    let destination = text(&trip.destination).size(16)
        .style(|_| text::Style { color: Some(Color::from_rgb(0.557, 0.141, 0.667)) }); // Travel purple
    let date_range = text(format!(
        "{}  –  {}",
        trip.start_date.format("%b %d"),
        trip.end_date.format("%b %d"),
    )).size(12).color(Color::from_rgb(0.46, 0.46, 0.46));

    let header = row![header_icon, destination, text("  ·  ").size(12), date_range]
        .spacing(4)
        .align_y(iced::Alignment::Center);

    // Reservation timeline — compact list of highlights
    let mut timeline_items: Vec<Element<Message>> = Vec::new();
    for highlight in &trip.highlights {
        timeline_items.push(highlight_card::highlight_card(highlight));
    }

    let thread_count = trip.thread_ids.len();
    let summary = text(format!("{} threads", thread_count))
        .size(12)
        .color(Color::from_rgb(0.62, 0.62, 0.62));

    let mut content = column![header].spacing(8).padding([12, 16]);
    for item in timeline_items {
        content = content.push(item);
    }
    content = content.push(summary);

    container(content)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(Color::WHITE)),
            border: iced::Border {
                radius: 4.0.into(),
                width: 1.0,
                color: Color::from_rgb(0.88, 0.88, 0.88),
            },
            shadow: iced::Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.08),
                offset: iced::Vector::new(0.0, 1.0),
                blur_radius: 3.0,
            },
            ..Default::default()
        })
        .width(Length::Fill)
        .into()
}
```

**Commit:** `feat(ui): add trip bundle card widget`

---

### Task 30 — Integrate highlights and trips into inbox feed

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/inbox.rs` (modify)

**Highlights integration:**

When rendering an `InboxItem::Thread`, check if the thread has highlights. If so, show highlight cards inline below the email row.

```rust
fn view_thread_row(&self, thread: &Thread) -> Element<Message> {
    let email_row = self.render_email_row(thread);

    let highlights = self.store.get_highlights_for_thread(&thread.id);
    if highlights.is_empty() {
        return email_row;
    }

    let mut col = column![email_row];
    // Show at most 3 highlights inline; rest behind "Show more"
    for h in highlights.iter().take(3) {
        col = col.push(
            container(highlight_card::highlight_card(h))
                .padding([0, 0, 0, 72]) // indent to align with email content (past avatar)
        );
    }
    if highlights.len() > 3 {
        col = col.push(
            button(text(format!("{} more highlights", highlights.len() - 3)).size(12))
                .on_press(Message::ShowAllHighlights(thread.id.clone()))
        );
    }

    col.spacing(2).into()
}
```

**Trip bundles integration:**

Include trip bundles in the feed. Trip bundles appear in the "Today" or "This Month" section based on their start date.

```rust
fn build_feed(&self) -> Vec<InboxItem> {
    let mut items = self.build_thread_feed(); // existing

    // Insert trip bundles
    let trips = self.store.get_trip_bundles();
    for trip in trips {
        items.push(InboxItem::TripBundle(trip));
    }

    // Sort by date (trips use start_date, threads use newest_date)
    items.sort_by(|a, b| b.sort_date().cmp(&a.sort_date()));

    items
}
```

In the feed renderer, handle `InboxItem::TripBundle`:

```rust
fn view_feed_item(&self, item: &InboxItem) -> Element<Message> {
    match item {
        InboxItem::Thread(t) => self.view_thread_row(t),
        InboxItem::Bundle(b) => self.view_bundle_row(b),
        InboxItem::Reminder(r) => self.view_reminder_row(r),
        InboxItem::TripBundle(trip) => trip_card::trip_bundle_card(trip),
    }
}
```

**Commit:** `feat(ui): integrate highlight cards and trip bundles into inbox feed`

---

## H. Integration Tests

### Task 31 — End-to-end extraction tests

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-extract/tests/integration.rs`

Full pipeline tests with realistic email content.

```rust
use inboxly_extract::{extract_highlights, ExtractInput};
use std::collections::HashMap;

#[test]
fn test_amazon_shipping_notification() {
    let input = ExtractInput {
        body_text: Some("Your order has shipped! \
            Carrier: UPS \
            Tracking Number: 1Z999AA10123456784 \
            Total: $49.99 \
            Thank you for your purchase.".to_string()),
        body_html: None,
        headers: HashMap::from([
            ("From".to_string(), "shipping@amazon.com".to_string()),
            ("Subject".to_string(), "Your order has shipped".to_string()),
        ]),
        subject: "Your order has shipped".to_string(),
        attachments: vec![],
    };

    let highlights = extract_highlights(&input);

    // Should find tracking number and payment
    assert!(highlights.iter().any(|h| matches!(h, inboxly_core::Highlight::TrackingNumber { carrier, .. } if carrier == "UPS")));
    assert!(highlights.iter().any(|h| matches!(h, inboxly_core::Highlight::Payment { amount, .. } if amount == "49.99")));
}

#[test]
fn test_airline_confirmation() {
    let input = ExtractInput {
        body_text: Some("Your flight itinerary \
            Flight: AC 123 \
            Departure: Toronto (YYZ) \
            Arrival: Vancouver (YVR) \
            Gate: B42 \
            Date: March 20, 2026".to_string()),
        body_html: None,
        headers: HashMap::from([
            ("Subject".to_string(), "Your Air Canada booking confirmation".to_string()),
        ]),
        subject: "Your Air Canada booking confirmation".to_string(),
        attachments: vec![],
    };

    let highlights = extract_highlights(&input);
    assert!(highlights.iter().any(|h| matches!(h, inboxly_core::Highlight::Flight { airline, .. } if airline == "AC")));
}

#[test]
fn test_calendar_invitation_with_ics() {
    let ics_data = b"BEGIN:VCALENDAR\r\n\
        BEGIN:VEVENT\r\n\
        SUMMARY:Team Standup\r\n\
        DTSTART:20260321T140000Z\r\n\
        LOCATION:Conference Room A\r\n\
        END:VEVENT\r\n\
        END:VCALENDAR\r\n";

    let input = ExtractInput {
        body_text: Some("You are invited to a meeting.".to_string()),
        body_html: None,
        headers: HashMap::new(),
        subject: "Meeting Invitation: Team Standup".to_string(),
        attachments: vec![("invite.ics".to_string(), ics_data.to_vec())],
    };

    let highlights = extract_highlights(&input);
    assert!(highlights.iter().any(|h| matches!(h, inboxly_core::Highlight::Event { title, .. } if title == "Team Standup")));
}

#[test]
fn test_personal_email_no_false_positives() {
    let input = ExtractInput {
        body_text: Some("Hey! Want to grab lunch tomorrow? Let me know.".to_string()),
        body_html: None,
        headers: HashMap::from([
            ("From".to_string(), "friend@example.com".to_string()),
        ]),
        subject: "Lunch tomorrow?".to_string(),
        attachments: vec![],
    };

    let highlights = extract_highlights(&input);
    assert!(highlights.is_empty(), "Personal email should produce no highlights");
}

#[test]
fn test_hotel_booking_confirmation() {
    let input = ExtractInput {
        body_text: Some("Your booking is confirmed! \
            Hotel: Grand Hyatt Vancouver \
            Confirmation Number: GH829315 \
            Check-in: March 20, 2026 \
            Check-out: March 23, 2026".to_string()),
        body_html: None,
        headers: HashMap::from([
            ("From".to_string(), "noreply@booking.com".to_string()),
        ]),
        subject: "Booking Confirmation - Grand Hyatt Vancouver".to_string(),
        attachments: vec![],
    };

    let highlights = extract_highlights(&input);
    assert!(highlights.iter().any(|h| matches!(h, inboxly_core::Highlight::Hotel { confirmation, .. } if confirmation == "GH829315")));
}
```

**Commit:** `test(extract): add end-to-end extraction integration tests`

---

### Task 32 — Multi-account and animation smoke tests

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/tests/smoke.rs`

Smoke tests for multi-account and animation features. These verify the state machines work correctly without requiring a running UI.

```rust
#[test]
fn test_animation_progress() {
    use inboxly_ui::animation::{Animation, easing};
    use std::time::Duration;

    let anim = Animation::start(Duration::from_millis(100));
    // Progress should be near 0.0 at start
    assert!(anim.progress() < 0.1);
    assert!(!anim.is_complete());

    std::thread::sleep(Duration::from_millis(110));
    assert!(anim.is_complete());
    assert!((anim.progress() - 1.0).abs() < 0.01);
}

#[test]
fn test_animation_easing_bounds() {
    use inboxly_ui::animation::easing;
    assert_eq!(easing::ease_out_cubic(0.0), 0.0);
    assert!((easing::ease_out_cubic(1.0) - 1.0).abs() < f32::EPSILON);
    assert_eq!(easing::linear(0.5), 0.5);
}

#[test]
fn test_sweep_state_cascade() {
    // Verify stagger timing produces correct order
    use inboxly_ui::animation::stagger_delay;
    use std::time::Duration;

    let d0 = stagger_delay(0, Duration::from_millis(50));
    let d1 = stagger_delay(1, Duration::from_millis(50));
    let d2 = stagger_delay(2, Duration::from_millis(50));

    assert_eq!(d0, Duration::from_millis(0));
    assert_eq!(d1, Duration::from_millis(50));
    assert_eq!(d2, Duration::from_millis(100));
}

#[test]
fn test_lerp_color() {
    use inboxly_ui::widgets::toolbar::lerp_color;
    use iced::Color;

    let black = Color::from_rgb(0.0, 0.0, 0.0);
    let white = Color::from_rgb(1.0, 1.0, 1.0);
    let mid = lerp_color(black, white, 0.5);
    assert!((mid.r - 0.5).abs() < 0.01);
    assert!((mid.g - 0.5).abs() < 0.01);
    assert!((mid.b - 0.5).abs() < 0.01);
}

#[test]
fn test_sync_manager_start_stop() {
    // Verify SyncManager tracks accounts correctly
    // (uses mock config, doesn't actually connect)
    let (tx, _rx) = tokio::sync::mpsc::channel(16);
    let mut manager = inboxly_imap::SyncManager::new(tx);
    assert!(manager.active_accounts().is_empty());
}
```

**Commit:** `test(ui): add animation and multi-account smoke tests`

---

## Build & Verify

```bash
cd /mnt/TempNVME/projects/inbox-rust

# Check all crates compile
cargo check --workspace

# Run all tests
cargo test --workspace

# Lint clean
cargo clippy --workspace -- -D warnings

# Specifically test extract crate
cargo test -p inboxly-extract
cargo test -p inboxly-store -- highlights
cargo test -p inboxly-store -- trips
cargo test -p inboxly-ui -- animation
```

---

## Commit Sequence

| # | Message | Section |
|---|---------|---------|
| 1 | `feat(extract): scaffold inboxly-extract crate with dependencies` | A |
| 2 | `feat(extract): add error type and extraction input struct` | A |
| 3 | `feat(extract): add tracking number extractor with carrier-specific patterns` | A |
| 4 | `feat(extract): add flight extractor with airline code validation` | A |
| 5 | `feat(extract): add hotel extractor with confirmation number detection` | A |
| 6 | `feat(extract): add event extractor with iCal and regex parsing` | A |
| 7 | `feat(extract): add payment extractor with currency amount detection` | A |
| 8 | `feat(extract): complete extraction pipeline with all 5 extractors` | A |
| 9 | `feat(store): ensure highlights table schema supports extraction pipeline` | B |
| 10 | `feat(store): add highlight CRUD operations` | B |
| 11 | `feat(store): integrate extraction pipeline into email ingest` | B |
| 12 | `feat(extract): add trip detection algorithm with date range clustering` | C |
| 13 | `feat(extract): add lenient date parsing for travel highlights` | C |
| 14 | `feat(store): add trip bundle detection, storage, and query API` | C |
| 15 | `feat(store): add account CRUD operations` | D |
| 16 | `feat(imap): add SyncManager for per-account independent sync tasks` | D |
| 17 | `feat(ui): add account add/remove settings UI` | D |
| 18 | `feat(ui): add account switcher dropdown to nav drawer` | D |
| 19 | `feat(ui): implement unified inbox feed for multi-account` | D |
| 20 | `feat(ui): add Inbox Zero Sun canvas widget` | E |
| 21 | `feat(ui): show Inbox Zero Sun when inbox feed is empty` | E |
| 22 | `feat(ui): add animation utilities with easing functions` | F |
| 23 | `feat(ui): animate bundle expand/collapse with 275ms ease-out` | F |
| 24 | `feat(ui): add sweep cascade animation with 50ms stagger` | F |
| 25 | `feat(ui): animate FAB speed dial with rotate and staggered fly-in` | F |
| 26 | `feat(ui): animate toolbar colour crossfade on view switch` | F |
| 27 | `feat(ui): animate swipe commit with slide-off and gap collapse` | F |
| 28 | `feat(ui): add highlight card widgets for all 5 highlight types` | G |
| 29 | `feat(ui): add trip bundle card widget` | G |
| 30 | `feat(ui): integrate highlight cards and trip bundles into inbox feed` | G |
| 31 | `test(extract): add end-to-end extraction integration tests` | H |
| 32 | `test(ui): add animation and multi-account smoke tests` | H |
