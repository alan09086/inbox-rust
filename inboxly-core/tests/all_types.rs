//! Integration test: verify all public types from inboxly-core compile
//! and can be instantiated together.

#![allow(unused_imports)]

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{NaiveDate, Utc};
use uuid::Uuid;

use inboxly_core::{
    // Identity types
    AccountId,
    // Contact and attachment
    Attachment,
    AttachmentMeta,
    // Bundle types
    Bundle,
    BundleCategory,
    BundleIcon,
    BundleId,
    BundleThrottle,
    BundleVisibility,
    // Trait types (verify they're importable)
    Bundler,
    Color,
    Contact,
    // Email types
    EmailContent,
    EmailFlags,
    EmailId,
    EmailMeta,
    Extractor,
    // Highlight types
    Highlight,
    // Inbox types
    InboxItem,
    // Error types
    InboxlyError,
    Result,
    SnoozeInfo,
    SnoozeUntil,
    Store,
    // Thread
    Thread,
    ThreadId,
    ThreadState,
    TripBundle,
};

#[test]
fn all_identity_types_constructable() {
    let _account = AccountId::new();
    let _email = EmailId::new("<test@example.com>");
    let _thread = ThreadId::new();
    let _bundle = BundleId::new();
}

#[test]
fn full_email_lifecycle() {
    let account_id = AccountId::new();
    let thread_id = ThreadId::new();
    let email_id = EmailId::new("<lifecycle@example.com>");

    // Create contact
    let sender = Contact::new("Alice", "alice@example.com");
    let recipient = Contact::new("Bob", "bob@example.com");

    // Create email metadata
    let meta = EmailMeta {
        id: email_id.clone(),
        account_id,
        thread_id,
        from: sender.clone(),
        to: vec![recipient.clone()],
        cc: vec![],
        subject: "Integration test email".into(),
        snippet: "This is a test...".into(),
        date: Utc::now(),
        maildir_path: PathBuf::from("/tmp/mail/cur/test:2,S"),
        attachments: vec![AttachmentMeta {
            filename: "doc.pdf".into(),
            mime_type: "application/pdf".into(),
            size_bytes: 2048,
        }],
        flags: EmailFlags {
            read: true,
            starred: false,
            answered: false,
            draft: false,
        },
        size_bytes: 8192,
        imap_uid: 100,
        imap_folder: "INBOX".into(),
    };

    assert_eq!(meta.subject, "Integration test email");
    assert!(meta.flags.read);

    // Create email content
    let content = EmailContent {
        id: email_id,
        body_text: Some("Plain text body".into()),
        body_html: Some("<p>HTML body</p>".into()),
        headers: HashMap::from([
            ("From".into(), "alice@example.com".into()),
            ("Subject".into(), "Integration test email".into()),
        ]),
        attachments: vec![Attachment {
            meta: meta.attachments[0].clone(),
            content: vec![0u8; 2048],
        }],
    };

    assert!(content.body_html.is_some());

    // Create thread
    let thread = Thread {
        id: thread_id,
        account_id,
        subject: meta.subject.clone(),
        participants: vec![sender, recipient],
        emails: vec![meta.id.clone()],
        newest_date: meta.date,
        oldest_date: meta.date,
        unread_count: 0,
        has_attachments: true,
        snippet: meta.snippet.clone(),
    };

    assert_eq!(thread.email_count(), 1);
    assert!(!thread.has_unread());
}

#[test]
fn full_bundle_lifecycle() {
    let bundle = Bundle {
        id: BundleId::new(),
        category: BundleCategory::Social,
        name: "Social".into(),
        color: Color::from_rgb_hex(0xd23f31),
        badge_color: Color::from_rgb_hex(0xfaebea),
        icon: BundleIcon::Social,
        threads: vec![ThreadId::new()],
        unread_count: 1,
        newest_date: Utc::now(),
        visibility: BundleVisibility::Bundled,
        throttle: BundleThrottle::Immediate,
    };

    assert_eq!(bundle.category.label(), "Social");

    // Wrap in InboxItem
    let item = InboxItem::Bundle(bundle);
    match &item {
        InboxItem::Bundle(b) => assert_eq!(b.name, "Social"),
        _ => panic!("expected Bundle"),
    }
}

#[test]
fn full_snooze_lifecycle() {
    let state = ThreadState {
        thread_id: ThreadId::new(),
        pinned: true,
        done: false,
        snoozed: Some(SnoozeInfo {
            until: SnoozeUntil::Time(Utc::now() + chrono::Duration::hours(4)),
            original_date: Utc::now(),
        }),
        bundle_id: Some(BundleId::new()),
        highlights: vec![Highlight::TrackingNumber {
            carrier: "UPS".into(),
            number: "1Z999".into(),
            url: None,
        }],
    };

    assert!(state.pinned);
    assert!(state.snoozed.is_some());
    assert_eq!(state.highlights.len(), 1);
}

#[test]
fn trip_bundle_assembly() {
    let trip = TripBundle {
        destination: "Vancouver".into(),
        start_date: NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
        end_date: NaiveDate::from_ymd_opt(2026, 7, 5).unwrap(),
        threads: vec![ThreadId::new()],
        highlights: vec![
            Highlight::Flight {
                airline: "WestJet".into(),
                number: "WS 100".into(),
                depart: Utc::now(),
                arrive: Utc::now() + chrono::Duration::hours(5),
                gate: Some("C12".into()),
            },
            Highlight::Hotel {
                name: "Fairmont Pacific Rim".into(),
                checkin: NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
                checkout: NaiveDate::from_ymd_opt(2026, 7, 5).unwrap(),
                confirmation: Some("FAIR-001".into()),
            },
        ],
    };

    let item = InboxItem::TripBundle(trip);
    match &item {
        InboxItem::TripBundle(t) => {
            assert_eq!(t.destination, "Vancouver");
            assert_eq!(t.highlights.len(), 2);
        }
        _ => panic!("expected TripBundle"),
    }
}

#[test]
fn error_types_usable() {
    fn failing_operation() -> Result<()> {
        Err(InboxlyError::EmailNotFound(EmailId::new(
            "<missing@mail.com>",
        )))
    }

    let err = failing_operation().unwrap_err();
    assert!(err.to_string().contains("email not found"));
}

#[test]
fn custom_bundle_category() {
    let custom = BundleCategory::Custom("Work Projects".into());
    assert_eq!(custom.label(), "Work Projects");
}

#[test]
fn email_flags_bitmask_storage() {
    let flags = EmailFlags {
        read: true,
        starred: true,
        answered: false,
        draft: false,
    };
    let mask = flags.to_bitmask();
    let restored = EmailFlags::from_bitmask(mask);
    assert_eq!(flags, restored);
}

#[test]
fn location_snooze() {
    let snooze = SnoozeUntil::Location {
        lat: 43.6532,
        lng: -79.3832,
        radius_m: 200.0,
        label: "Home".into(),
    };
    match &snooze {
        SnoozeUntil::Location {
            label, radius_m, ..
        } => {
            assert_eq!(label, "Home");
            assert!(*radius_m > 0.0);
        }
        _ => panic!("expected Location"),
    }
}
