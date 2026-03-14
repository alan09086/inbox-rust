use inboxly_imap::sync::batch::BatchIterator;

#[test]
fn batch_iterator_small_mailbox() {
    // 50 messages, UIDs 1..=50, batch size 500
    // Should produce one batch: 1:50
    let batches: Vec<_> = BatchIterator::new(1, 51, 500).collect();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0], (1, 50)); // (start, end) inclusive
}

#[test]
fn batch_iterator_exact_multiple() {
    // 1000 messages, UIDs 1..=1000, batch size 500
    // Newest-first: batch 1 = 501:1000, batch 2 = 1:500
    let batches: Vec<_> = BatchIterator::new(1, 1001, 500).collect();
    assert_eq!(batches.len(), 2);
    assert_eq!(batches[0], (501, 1000)); // newest first
    assert_eq!(batches[1], (1, 500));
}

#[test]
fn batch_iterator_large_mailbox() {
    // 1250 messages, UIDs 1..=1250, batch size 500
    // batch 1 = 751:1250, batch 2 = 251:750, batch 3 = 1:250
    let batches: Vec<_> = BatchIterator::new(1, 1251, 500).collect();
    assert_eq!(batches.len(), 3);
    assert_eq!(batches[0], (751, 1250));
    assert_eq!(batches[1], (251, 750));
    assert_eq!(batches[2], (1, 250));
}

#[test]
fn batch_iterator_non_contiguous_uids() {
    // UIDs may not start at 1 — e.g., after deletions.
    // lowest_uid=500, uid_next=1800, batch_size=500
    // Range is 500..=1799 (1300 UIDs)
    // batch 1 = 1300:1799, batch 2 = 800:1299, batch 3 = 500:799
    let batches: Vec<_> = BatchIterator::new(500, 1800, 500).collect();
    assert_eq!(batches.len(), 3);
    assert_eq!(batches[0], (1300, 1799));
    assert_eq!(batches[1], (800, 1299));
    assert_eq!(batches[2], (500, 799));
}

#[test]
fn batch_iterator_empty_mailbox() {
    // uid_next=1 means no messages
    let batches: Vec<_> = BatchIterator::new(1, 1, 500).collect();
    assert_eq!(batches.len(), 0);
}

#[test]
fn batch_iterator_single_message() {
    // One message, UID=1, uid_next=2
    let batches: Vec<_> = BatchIterator::new(1, 2, 500).collect();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0], (1, 1));
}

#[test]
fn batch_iterator_resume_from_uid() {
    // Resuming after crash: already synced UIDs 501..=1000.
    // Resume from lowest_uid=1, up to resume_uid=500 (exclusive of already-done).
    // This simulates using BatchIterator with a truncated range.
    let batches: Vec<_> = BatchIterator::new(1, 501, 500).collect();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0], (1, 500));
}

#[test]
fn batch_to_imap_sequence_string() {
    use inboxly_imap::sync::batch::batch_to_sequence;
    assert_eq!(batch_to_sequence(501, 1000), "501:1000");
    assert_eq!(batch_to_sequence(1, 1), "1");
}
