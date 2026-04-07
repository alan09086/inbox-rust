//! Integration tests for the threading algorithm using the full Store API.
//!
//! These tests verify realistic email threading scenarios end-to-end.

use inboxly_store::*;

fn test_store() -> Store {
    Store::open_in_memory().expect("failed to open in-memory store")
}

fn sample_account() -> AccountRow {
    AccountRow {
        id: "acct-001".into(),
        email: "alice@example.com".into(),
        display_name: "Alice".into(),
        provider: "generic".into(),
        auth_method: "password".into(),
        imap_host: "imap.example.com".into(),
        imap_port: 993,
        smtp_host: "smtp.example.com".into(),
        smtp_port: 587,
    }
}

fn make_email(
    id: &str,
    message_id: &str,
    in_reply_to: Option<&str>,
    references: &[&str],
    subject: &str,
    date: i64,
    uid: i64,
) -> EmailRow {
    let refs_json = if references.is_empty() {
        None
    } else {
        Some(serde_json::to_string(references).expect("serialize refs"))
    };

    EmailRow {
        id: id.into(),
        account_id: "acct-001".into(),
        thread_id: String::new(), // Empty = unthreaded.
        from_name: Some("Sender".into()),
        from_address: "sender@example.com".into(),
        to_json: r#"[{"name":"Recipient","address":"recipient@example.com"}]"#.into(),
        cc_json: "[]".into(),
        subject: subject.into(),
        snippet: format!("Snippet for {subject}"),
        date,
        maildir_path: String::new(),
        flags: 0,
        size_bytes: 1024,
        imap_uid: uid,
        imap_folder: "INBOX".into(),
        has_attachments: false,
        body_downloaded: false,
        message_id_header: Some(message_id.into()),
        in_reply_to: in_reply_to.map(String::from),
        references_json: refs_json,
    }
}

/// Helper: insert emails, run batch threading, return the store.
fn setup_and_thread(emails: Vec<EmailRow>) -> Store {
    let mut store = test_store();
    store.insert_account(&sample_account()).unwrap();

    // emails.thread_id has no FK constraint, so empty string is fine.
    for email in &emails {
        store.insert_email(email).unwrap();
    }

    // Run batch threading.
    store
        .transaction(|conn| {
            inboxly_store::threading::thread_unthreaded_emails(conn, "acct-001")?;
            Ok(())
        })
        .unwrap();

    store
}

/// Get the thread_id for an email from the store.
fn get_thread_id(store: &Store, email_id: &str) -> String {
    store.get_email(email_id).unwrap().thread_id
}

// === Test scenarios ===

#[test]
fn simple_two_email_thread() {
    let emails = vec![
        make_email("e1", "orig@ex.com", None, &[], "Hello", 1710000000, 1),
        make_email(
            "e2",
            "reply@ex.com",
            Some("orig@ex.com"),
            &["orig@ex.com"],
            "Re: Hello",
            1710001000,
            2,
        ),
    ];
    let store = setup_and_thread(emails);

    let t1 = get_thread_id(&store, "e1");
    let t2 = get_thread_id(&store, "e2");
    assert_eq!(t1, t2);

    // Verify thread metadata.
    let thread = store.get_thread(&t1).unwrap();
    assert_eq!(thread.email_count, 2);
    assert_eq!(thread.subject, "Hello"); // From oldest.
    assert_eq!(thread.snippet, "Snippet for Re: Hello"); // From newest.
}

#[test]
fn three_level_thread() {
    let emails = vec![
        make_email("e1", "orig@ex.com", None, &[], "Topic", 1710000000, 1),
        make_email(
            "e2",
            "reply1@ex.com",
            Some("orig@ex.com"),
            &["orig@ex.com"],
            "Re: Topic",
            1710001000,
            2,
        ),
        make_email(
            "e3",
            "reply2@ex.com",
            Some("reply1@ex.com"),
            &["orig@ex.com", "reply1@ex.com"],
            "Re: Re: Topic",
            1710002000,
            3,
        ),
    ];
    let store = setup_and_thread(emails);

    let t1 = get_thread_id(&store, "e1");
    let t2 = get_thread_id(&store, "e2");
    let t3 = get_thread_id(&store, "e3");
    assert_eq!(t1, t2);
    assert_eq!(t2, t3);

    let thread = store.get_thread(&t1).unwrap();
    assert_eq!(thread.email_count, 3);
}

#[test]
fn branching_thread() {
    let emails = vec![
        make_email("e1", "orig@ex.com", None, &[], "Topic", 1710000000, 1),
        make_email(
            "e2",
            "branch-a@ex.com",
            Some("orig@ex.com"),
            &["orig@ex.com"],
            "Re: Topic (branch A)",
            1710001000,
            2,
        ),
        make_email(
            "e3",
            "branch-b@ex.com",
            Some("orig@ex.com"),
            &["orig@ex.com"],
            "Re: Topic (branch B)",
            1710002000,
            3,
        ),
    ];
    let store = setup_and_thread(emails);

    let t1 = get_thread_id(&store, "e1");
    let t2 = get_thread_id(&store, "e2");
    let t3 = get_thread_id(&store, "e3");
    assert_eq!(t1, t2);
    assert_eq!(t2, t3);
}

#[test]
fn reply_before_parent() {
    // Reply arrives BEFORE parent (parent has later date in References but
    // we process oldest first, so reply (date 1710001000) is after root
    // (date 1710000000). But here reply actually arrives first in insertion
    // order with an earlier date to test placeholder creation).
    let emails = vec![
        // Reply first (earlier UID but later date for realistic ordering).
        make_email(
            "e-reply",
            "reply@ex.com",
            Some("orig@ex.com"),
            &["orig@ex.com"],
            "Re: Hello",
            1710001000,
            2,
        ),
        // Parent arrives second (earlier date).
        make_email("e-orig", "orig@ex.com", None, &[], "Hello", 1710000000, 1),
    ];
    let store = setup_and_thread(emails);

    let t_reply = get_thread_id(&store, "e-reply");
    let t_orig = get_thread_id(&store, "e-orig");
    assert_eq!(t_reply, t_orig);

    let thread = store.get_thread(&t_orig).unwrap();
    assert_eq!(thread.email_count, 2);
    assert_eq!(thread.subject, "Hello"); // From oldest date.
    assert_eq!(thread.snippet, "Snippet for Re: Hello"); // From newest date.
}

#[test]
fn deep_reply_chain_out_of_order() {
    // Insert emails 5, 3, 1, 4, 2 (out of date order).
    // But batch threading processes oldest-first, so actual order is 1,2,3,4,5.
    let emails = vec![
        make_email(
            "e5",
            "msg5@ex.com",
            Some("msg4@ex.com"),
            &["msg1@ex.com", "msg2@ex.com", "msg3@ex.com", "msg4@ex.com"],
            "Re: Chain 5",
            1710004000,
            5,
        ),
        make_email(
            "e3",
            "msg3@ex.com",
            Some("msg2@ex.com"),
            &["msg1@ex.com", "msg2@ex.com"],
            "Re: Chain 3",
            1710002000,
            3,
        ),
        make_email("e1", "msg1@ex.com", None, &[], "Chain", 1710000000, 1),
        make_email(
            "e4",
            "msg4@ex.com",
            Some("msg3@ex.com"),
            &["msg1@ex.com", "msg2@ex.com", "msg3@ex.com"],
            "Re: Chain 4",
            1710003000,
            4,
        ),
        make_email(
            "e2",
            "msg2@ex.com",
            Some("msg1@ex.com"),
            &["msg1@ex.com"],
            "Re: Chain 2",
            1710001000,
            2,
        ),
    ];
    let store = setup_and_thread(emails);

    // All should be in the same thread.
    let t1 = get_thread_id(&store, "e1");
    let t2 = get_thread_id(&store, "e2");
    let t3 = get_thread_id(&store, "e3");
    let t4 = get_thread_id(&store, "e4");
    let t5 = get_thread_id(&store, "e5");
    assert_eq!(t1, t2);
    assert_eq!(t2, t3);
    assert_eq!(t3, t4);
    assert_eq!(t4, t5);

    let thread = store.get_thread(&t1).unwrap();
    assert_eq!(thread.email_count, 5);
    assert_eq!(thread.newest_date, 1710004000);
    assert_eq!(thread.oldest_date, 1710000000);
}

#[test]
fn multiple_independent_threads() {
    let emails = vec![
        make_email(
            "e1",
            "standalone1@ex.com",
            None,
            &[],
            "Topic A",
            1710000000,
            1,
        ),
        make_email(
            "e2",
            "standalone2@ex.com",
            None,
            &[],
            "Topic B",
            1710001000,
            2,
        ),
        make_email(
            "e3",
            "standalone3@ex.com",
            None,
            &[],
            "Topic C",
            1710002000,
            3,
        ),
        make_email(
            "e4",
            "standalone4@ex.com",
            None,
            &[],
            "Topic D",
            1710003000,
            4,
        ),
        make_email(
            "e5",
            "standalone5@ex.com",
            None,
            &[],
            "Topic E",
            1710004000,
            5,
        ),
    ];
    let store = setup_and_thread(emails);

    let t1 = get_thread_id(&store, "e1");
    let t2 = get_thread_id(&store, "e2");
    let t3 = get_thread_id(&store, "e3");
    let t4 = get_thread_id(&store, "e4");
    let t5 = get_thread_id(&store, "e5");

    // All different threads.
    let unique: std::collections::HashSet<_> = [&t1, &t2, &t3, &t4, &t5].into_iter().collect();
    assert_eq!(unique.len(), 5);
}

#[test]
fn mixed_threads_and_standalone() {
    let emails = vec![
        // 3-email thread.
        make_email("e1", "root@ex.com", None, &[], "Thread", 1710000000, 1),
        make_email(
            "e2",
            "reply1@ex.com",
            Some("root@ex.com"),
            &["root@ex.com"],
            "Re: Thread",
            1710001000,
            2,
        ),
        make_email(
            "e3",
            "reply2@ex.com",
            Some("reply1@ex.com"),
            &["root@ex.com", "reply1@ex.com"],
            "Re: Re: Thread",
            1710002000,
            3,
        ),
        // 2 standalone emails.
        make_email("e4", "solo1@ex.com", None, &[], "Solo A", 1710003000, 4),
        make_email("e5", "solo2@ex.com", None, &[], "Solo B", 1710004000, 5),
    ];
    let store = setup_and_thread(emails);

    let t1 = get_thread_id(&store, "e1");
    let t2 = get_thread_id(&store, "e2");
    let t3 = get_thread_id(&store, "e3");
    let t4 = get_thread_id(&store, "e4");
    let t5 = get_thread_id(&store, "e5");

    // e1, e2, e3 in same thread.
    assert_eq!(t1, t2);
    assert_eq!(t2, t3);

    // e4 and e5 in different threads.
    assert_ne!(t4, t5);
    assert_ne!(t4, t1);
    assert_ne!(t5, t1);
}

#[test]
fn gmail_style_references() {
    // Gmail includes full References chain.
    let emails = vec![
        make_email("e1", "a@ex.com", None, &[], "Topic", 1710000000, 1),
        make_email(
            "e2",
            "b@ex.com",
            Some("a@ex.com"),
            &["a@ex.com"],
            "Re: Topic",
            1710001000,
            2,
        ),
        make_email(
            "e3",
            "c@ex.com",
            Some("b@ex.com"),
            &["a@ex.com", "b@ex.com"],
            "Re: Topic",
            1710002000,
            3,
        ),
        make_email(
            "e4",
            "d@ex.com",
            Some("c@ex.com"),
            &["a@ex.com", "b@ex.com", "c@ex.com"],
            "Re: Topic",
            1710003000,
            4,
        ),
    ];
    let store = setup_and_thread(emails);

    // All in one thread (root = a@ex.com).
    let t1 = get_thread_id(&store, "e1");
    let t2 = get_thread_id(&store, "e2");
    let t3 = get_thread_id(&store, "e3");
    let t4 = get_thread_id(&store, "e4");
    assert_eq!(t1, t2);
    assert_eq!(t2, t3);
    assert_eq!(t3, t4);
}

#[test]
fn cross_post_same_root() {
    // Two replies from different mailing lists, same root.
    let emails = vec![
        make_email("e1", "root@ex.com", None, &[], "Discussion", 1710000000, 1),
        make_email(
            "e2",
            "list-a-reply@ex.com",
            Some("root@ex.com"),
            &["root@ex.com", "list-a@ex.com"],
            "Re: Discussion [list-a]",
            1710001000,
            2,
        ),
        make_email(
            "e3",
            "list-b-reply@ex.com",
            Some("root@ex.com"),
            &["root@ex.com", "list-b@ex.com"],
            "Re: Discussion [list-b]",
            1710002000,
            3,
        ),
    ];
    let store = setup_and_thread(emails);

    // Both replies share root@ex.com as References[0], so same thread.
    let t1 = get_thread_id(&store, "e1");
    let t2 = get_thread_id(&store, "e2");
    let t3 = get_thread_id(&store, "e3");
    assert_eq!(t1, t2);
    assert_eq!(t2, t3);
}

#[test]
fn rebuild_preserves_thread_membership() {
    let emails = vec![
        // Thread 1.
        make_email("e1", "root1@ex.com", None, &[], "Thread 1", 1710000000, 1),
        make_email(
            "e2",
            "reply1@ex.com",
            Some("root1@ex.com"),
            &["root1@ex.com"],
            "Re: Thread 1",
            1710001000,
            2,
        ),
        // Thread 2.
        make_email("e3", "root2@ex.com", None, &[], "Thread 2", 1710002000, 3),
        make_email(
            "e4",
            "reply2@ex.com",
            Some("root2@ex.com"),
            &["root2@ex.com"],
            "Re: Thread 2",
            1710003000,
            4,
        ),
        // Standalone.
        make_email("e5", "solo@ex.com", None, &[], "Solo", 1710004000, 5),
    ];
    let store = setup_and_thread(emails);

    // Record pre-rebuild groupings.
    let pre_t1 = get_thread_id(&store, "e1");
    let pre_t2 = get_thread_id(&store, "e2");
    let pre_t3 = get_thread_id(&store, "e3");
    let pre_t4 = get_thread_id(&store, "e4");
    assert_eq!(pre_t1, pre_t2);
    assert_eq!(pre_t3, pre_t4);

    // Rebuild.
    let mut store = store;
    store
        .transaction(|conn| {
            inboxly_store::threading::rebuild_threads(conn, "acct-001")?;
            Ok(())
        })
        .unwrap();

    // Post-rebuild: same groupings, but thread IDs may differ.
    let post_t1 = get_thread_id(&store, "e1");
    let post_t2 = get_thread_id(&store, "e2");
    let post_t3 = get_thread_id(&store, "e3");
    let post_t4 = get_thread_id(&store, "e4");
    let post_t5 = get_thread_id(&store, "e5");

    assert_eq!(post_t1, post_t2); // Same group.
    assert_eq!(post_t3, post_t4); // Same group.
    assert_ne!(post_t1, post_t3); // Different groups.
    assert_ne!(post_t5, post_t1); // Standalone.
    assert_ne!(post_t5, post_t3); // Standalone.
}

#[test]
fn thread_metadata_after_flag_change() {
    let emails = vec![
        make_email("e1", "root@ex.com", None, &[], "Topic", 1710000000, 1),
        make_email(
            "e2",
            "reply1@ex.com",
            Some("root@ex.com"),
            &["root@ex.com"],
            "Re: Topic",
            1710001000,
            2,
        ),
        make_email(
            "e3",
            "reply2@ex.com",
            Some("root@ex.com"),
            &["root@ex.com"],
            "Re: Topic",
            1710002000,
            3,
        ),
    ];
    let mut store = setup_and_thread(emails);

    let tid = get_thread_id(&store, "e1");
    let thread = store.get_thread(&tid).unwrap();
    assert_eq!(thread.unread_count, 3); // All unread.

    // Mark one as read.
    store.update_email_flags("e1", flags::READ).unwrap();

    // Refresh metadata.
    store
        .transaction(|conn| {
            inboxly_store::threading::refresh_thread_metadata(conn, &tid)?;
            Ok(())
        })
        .unwrap();

    let thread = store.get_thread(&tid).unwrap();
    assert_eq!(thread.unread_count, 2);
}

#[test]
fn batch_threading_performance() {
    let store = test_store();
    store.insert_account(&sample_account()).unwrap();

    // Insert 500 emails in chains (100 chains of 5) + 500 standalone.
    let mut uid = 1i64;
    for chain in 0..100 {
        let root_mid = format!("root-{chain}@ex.com");
        let root = make_email(
            &format!("chain-{chain}-0"),
            &root_mid,
            None,
            &[],
            &format!("Chain {chain}"),
            1710000000 + (chain * 10),
            uid,
        );
        store.insert_email(&root).unwrap();
        uid += 1;

        for reply in 1..5 {
            let reply_mid = format!("chain-{chain}-reply-{reply}@ex.com");
            let email = make_email(
                &format!("chain-{chain}-{reply}"),
                &reply_mid,
                Some(&root_mid),
                &[&root_mid],
                &format!("Re: Chain {chain}"),
                1710000000 + (chain * 10) + reply,
                uid,
            );
            store.insert_email(&email).unwrap();
            uid += 1;
        }
    }

    for i in 0..500 {
        let email = make_email(
            &format!("solo-{i}"),
            &format!("solo-{i}@ex.com"),
            None,
            &[],
            &format!("Solo {i}"),
            1710010000 + i,
            uid,
        );
        store.insert_email(&email).unwrap();
        uid += 1;
    }

    let start = std::time::Instant::now();
    let mut store = store;
    store
        .transaction(|conn| {
            let count = inboxly_store::threading::thread_unthreaded_emails(conn, "acct-001")?;
            assert_eq!(count, 1000);
            Ok(())
        })
        .unwrap();
    let elapsed = start.elapsed();

    // Should complete in under 2 seconds (generous for CI).
    assert!(
        elapsed.as_secs() < 2,
        "batch threading took {elapsed:?}, expected < 2s"
    );

    // Verify correct thread count: 100 chains + 500 standalone = 600 threads.
    let thread_count: i64 = store
        .transaction(|conn| {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM threads WHERE account_id = 'acct-001'",
                    [],
                    |row| row.get(0),
                )
                .map_err(inboxly_store::StoreError::from)?;
            Ok(count)
        })
        .unwrap();
    assert_eq!(thread_count, 600);
}

#[test]
fn concurrent_account_threading() {
    let store = test_store();
    store.insert_account(&sample_account()).unwrap();
    store
        .insert_account(&AccountRow {
            id: "acct-002".into(),
            email: "bob@example.com".into(),
            display_name: "Bob".into(),
            provider: "generic".into(),
            auth_method: "password".into(),
            imap_host: "imap.example.com".into(),
            imap_port: 993,
            smtp_host: "smtp.example.com".into(),
            smtp_port: 587,
        })
        .unwrap();

    // Account 1 emails.
    let mut e1 = make_email(
        "a1-e1",
        "a1-root@ex.com",
        None,
        &[],
        "Account 1",
        1710000000,
        1,
    );
    e1.account_id = "acct-001".into();
    store.insert_email(&e1).unwrap();

    let mut e2 = make_email(
        "a1-e2",
        "a1-reply@ex.com",
        Some("a1-root@ex.com"),
        &["a1-root@ex.com"],
        "Re: Account 1",
        1710001000,
        2,
    );
    e2.account_id = "acct-001".into();
    store.insert_email(&e2).unwrap();

    // Account 2 emails.
    let mut e3 = make_email(
        "a2-e1",
        "a2-root@ex.com",
        None,
        &[],
        "Account 2",
        1710000000,
        1,
    );
    e3.account_id = "acct-002".into();
    store.insert_email(&e3).unwrap();

    // Thread both accounts.
    let mut store = store;
    store
        .transaction(|conn| {
            inboxly_store::threading::thread_unthreaded_emails(conn, "acct-001")?;
            inboxly_store::threading::thread_unthreaded_emails(conn, "acct-002")?;
            Ok(())
        })
        .unwrap();

    // Verify no cross-account contamination.
    let t1 = store.get_email("a1-e1").unwrap().thread_id;
    let t2 = store.get_email("a1-e2").unwrap().thread_id;
    let t3 = store.get_email("a2-e1").unwrap().thread_id;

    assert_eq!(t1, t2); // Same account, same thread.
    assert_ne!(t1, t3); // Different accounts, different threads.
}

/// M34 phase 4 prep: regression test for `Store::get_emails_by_thread`
/// returning rows in chronological (date ASC) order.
///
/// Insertion order is deliberately scrambled relative to chronological
/// order — the earliest-dated email is inserted last. Without
/// `ORDER BY date ASC` in the query, this test would observe
/// insertion order (or worse, ROWID order) and fail.
#[test]
fn get_emails_by_thread_returns_chronological_order() {
    let store = test_store();
    store.insert_account(&sample_account()).unwrap();

    // Three emails with the SAME (manually-set) thread_id but
    // out-of-order dates. We bypass the threading algorithm and
    // assign thread_id directly so the test focuses purely on the
    // ordering of `get_emails_by_thread`.
    let thread_id = "t-ord";
    let mut e1 = make_email("e1", "e1@ex.com", None, &[], "Latest", 3000, 1);
    e1.thread_id = thread_id.into();
    let mut e2 = make_email("e2", "e2@ex.com", None, &[], "Earliest", 1000, 2);
    e2.thread_id = thread_id.into();
    let mut e3 = make_email("e3", "e3@ex.com", None, &[], "Middle", 2000, 3);
    e3.thread_id = thread_id.into();

    store.insert_email(&e1).unwrap();
    store.insert_email(&e2).unwrap();
    store.insert_email(&e3).unwrap();

    let rows = store.get_emails_by_thread(thread_id).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].id, "e2", "earliest date should be first");
    assert_eq!(rows[1].id, "e3", "middle date should be second");
    assert_eq!(rows[2].id, "e1", "latest date should be last");
}
