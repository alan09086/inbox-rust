# M10: Threading Algorithm

**Crate**: `inboxly-store`
**Branch**: `m10-threading-algorithm`
**Prereqs**: M3 (SQLite schema + store API), M7 (initial sync phase 1 — emails populated)
**Spec ref**: Design spec §Threading Algorithm (lines 229-238)

---

## Overview

This milestone implements References-based email threading (simplified JWZ) inside `inboxly-store`. Every email ingested into the store gets assigned a `ThreadId` based on its `Message-ID`, `In-Reply-To`, and `References` headers. The algorithm creates placeholder threads for orphaned replies, unifies threads when parents arrive, and maintains aggregated thread metadata (newest_date, unread_count, snippet, participants). A re-threading function allows rebuilding all threads from scratch.

**Explicit design decision**: No subject-based grouping. This avoids false positives (e.g., multiple unrelated "Re: Hello" threads).

---

## Tasks

### Task 1 — Header extraction utility

**File**: `inboxly-store/src/threading/headers.rs` (new)

Create a module that parses threading-related headers from raw email header maps.

**Types**:

```rust
/// Parsed threading headers from a single email.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadingHeaders {
    pub message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
}
```

**Functions**:

```rust
/// Extract threading headers from a header map.
/// - `Message-ID`: strip angle brackets, normalize whitespace
/// - `In-Reply-To`: strip angle brackets, take only the first Message-ID if multiple
/// - `References`: split on whitespace, strip angle brackets from each, preserve order
///
/// All Message-IDs are trimmed and lowercased for consistent lookups.
pub fn extract_threading_headers(headers: &HashMap<String, String>) -> ThreadingHeaders;

/// Parse a single Message-ID value: strip angle brackets, trim, lowercase.
/// Returns None if the input is empty or only whitespace after stripping.
fn parse_message_id(raw: &str) -> Option<String>;

/// Parse a References header value into an ordered list of Message-IDs.
/// References is a space-separated list of angle-bracket-delimited Message-IDs.
/// Example: "<a@example.com> <b@example.com> <c@example.com>"
fn parse_references(raw: &str) -> Vec<String>;
```

**Implementation notes**:
- Use a regex or manual parser to extract `<...>` tokens from References. A simple approach: split on `>`, then for each piece find the content after `<`.
- Header names are case-insensitive per RFC 2822. The header map from M7 should already use canonical casing, but match case-insensitively to be safe: check for `"Message-ID"`, `"Message-Id"`, `"message-id"`, etc. Best approach: iterate the map and compare `key.to_ascii_lowercase()`.
- Malformed Message-IDs (no `@` sign, empty string) should be preserved as-is after bracket-stripping — don't reject them, since real-world emails have all kinds of broken IDs. Only return `None` from `parse_message_id` if the result is truly empty.

**Tests** (`inboxly-store/src/threading/headers_tests.rs` or inline `#[cfg(test)]`):

1. Standard headers: `Message-ID: <abc@example.com>`, `References: <root@ex.com> <mid@ex.com>` → correct extraction
2. Missing all headers → `ThreadingHeaders { message_id: None, in_reply_to: None, references: vec![] }`
3. Only `In-Reply-To` present
4. `References` with inconsistent whitespace (tabs, newlines from header folding)
5. Angle brackets missing (bare IDs) — should still parse
6. Multiple `In-Reply-To` values (take first only)
7. Case-insensitive header name lookup (`message-id` vs `Message-ID`)
8. Duplicate Message-IDs in References — preserve duplicates (don't dedup, downstream handles it)

**Commit**: `feat(store): add threading header extraction utility`

---

### Task 2 — Thread assignment for a single email (core algorithm)

**File**: `inboxly-store/src/threading/assign.rs` (new)

This is the core threading algorithm. Given an email's parsed `ThreadingHeaders`, determine which `ThreadId` the email should belong to.

**Function signature**:

```rust
use rusqlite::Connection;
use uuid::Uuid;

/// Determine the ThreadId for an email based on its threading headers.
///
/// Algorithm:
/// 1. If `References` is non-empty, the thread root is `references[0]` (the original message).
///    Look up an existing thread that has an email with that Message-ID.
///    - If found, return that thread's ThreadId.
///    - If not found, create a placeholder thread keyed on that root Message-ID and return it.
///
/// 2. If `References` is empty but `In-Reply-To` is present, look up the email with
///    that Message-ID and return its ThreadId.
///    - If the referenced email doesn't exist yet, create a placeholder thread
///      keyed on the In-Reply-To Message-ID and return it.
///
/// 3. If neither header is present, create a new thread with a fresh UUID and return it.
///
/// Returns (ThreadId, bool) — the thread ID and whether a new thread was created.
pub fn assign_thread(
    conn: &Connection,
    account_id: &str,
    headers: &ThreadingHeaders,
    email_subject: &str,
    email_date: i64,
) -> Result<(ThreadId, bool), StoreError>;
```

**Helper queries** (use `rusqlite` directly):

```rust
/// Look up the thread_id of an email by its message_id_header column.
/// Returns None if no email with that Message-ID exists.
fn find_thread_by_message_id(conn: &Connection, message_id: &str) -> Result<Option<String>, StoreError>;

/// Look up a placeholder thread by its root_message_id.
/// Placeholder threads are identified by having root_message_id set in the threads table.
fn find_placeholder_thread(conn: &Connection, root_message_id: &str) -> Result<Option<String>, StoreError>;

/// Create a new thread row in the threads table.
/// If `root_message_id` is Some, this is a placeholder thread awaiting the root email.
fn create_thread(
    conn: &Connection,
    thread_id: &str,
    account_id: &str,
    subject: &str,
    date: i64,
    root_message_id: Option<&str>,
) -> Result<(), StoreError>;
```

**Schema addition** — the `threads` table from M3 needs one new column:

```sql
ALTER TABLE threads ADD COLUMN root_message_id TEXT;
CREATE INDEX idx_threads_root_message_id ON threads(root_message_id);
```

This column maps a thread to the Message-ID of its root email. It's `NULL` for threads that already contain their root email; it's set for placeholder threads awaiting the root. Also used for fast lookup during thread assignment.

Add this migration to the store's migration system (the M3 schema init or a new migration step, depending on how M3 structured migrations). If M3 uses a `user_version` pragma approach:

```rust
// Migration from version N to N+1
if current_version < THREADING_VERSION {
    conn.execute_batch("
        ALTER TABLE threads ADD COLUMN root_message_id TEXT;
        CREATE INDEX IF NOT EXISTS idx_threads_root_message_id ON threads(root_message_id);
    ")?;
    conn.pragma_update(None, "user_version", THREADING_VERSION)?;
}
```

**Algorithm detail** (pseudocode):

```
fn assign_thread(conn, account_id, headers, subject, date):
    // Case 1: References present
    if !headers.references.is_empty():
        root_mid = headers.references[0]

        // Check if an email with this root Message-ID already exists
        if let Some(tid) = find_thread_by_message_id(conn, root_mid):
            return (tid, false)

        // Check if a placeholder thread exists for this root
        if let Some(tid) = find_placeholder_thread(conn, root_mid):
            return (tid, false)

        // No thread exists — create placeholder
        new_tid = Uuid::new_v4().to_string()
        create_thread(conn, new_tid, account_id, subject, date, Some(root_mid))
        return (new_tid, true)

    // Case 2: In-Reply-To only
    if let Some(irt) = &headers.in_reply_to:
        if let Some(tid) = find_thread_by_message_id(conn, irt):
            return (tid, false)

        if let Some(tid) = find_placeholder_thread(conn, irt):
            return (tid, false)

        // Create placeholder keyed on In-Reply-To
        new_tid = Uuid::new_v4().to_string()
        create_thread(conn, new_tid, account_id, subject, date, Some(irt))
        return (new_tid, true)

    // Case 3: No threading headers — new standalone thread
    new_tid = Uuid::new_v4().to_string()
    create_thread(conn, new_tid, account_id, subject, date, None)
    return (new_tid, true)
```

**Tests** (in-memory SQLite):

1. Email with no threading headers → new thread created, `root_message_id` is NULL
2. Email with References `[A, B, C]` where `A` exists → joins A's thread
3. Email with References `[A, B, C]` where `A` doesn't exist → creates placeholder with `root_message_id = A`
4. Second email with References `[A, B, D]` where placeholder for `A` exists → joins the same placeholder thread
5. Email with only In-Reply-To pointing to existing email → joins that thread
6. Email with only In-Reply-To pointing to non-existent email → creates placeholder
7. Email with both References and In-Reply-To → References takes precedence (In-Reply-To ignored)

**Commit**: `feat(store): implement core thread assignment algorithm`

---

### Task 3 — Placeholder thread creation and tracking

**File**: `inboxly-store/src/threading/assign.rs` (extend from Task 2)

The placeholder mechanism is built into Task 2's `create_thread` function. This task focuses on ensuring placeholder threads are properly identifiable and queryable.

**Functions**:

```rust
/// Check if a thread is a placeholder (root email hasn't arrived yet).
pub fn is_placeholder_thread(conn: &Connection, thread_id: &str) -> Result<bool, StoreError> {
    // A placeholder has root_message_id set AND no email in the thread
    // has message_id_header == root_message_id
    let has_root: bool = conn.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM threads t
            JOIN emails e ON e.thread_id = t.id AND e.message_id_header = t.root_message_id
            WHERE t.id = ?1
        )",
        [thread_id],
        |row| row.get(0),
    )?;
    let has_root_mid: bool = conn.query_row(
        "SELECT root_message_id IS NOT NULL FROM threads WHERE id = ?1",
        [thread_id],
        |row| row.get(0),
    )?;
    Ok(has_root_mid && !has_root)
}

/// List all placeholder thread IDs (for diagnostics/re-threading).
pub fn list_placeholder_threads(conn: &Connection) -> Result<Vec<String>, StoreError>;
```

**Tests**:

1. Create placeholder thread → `is_placeholder_thread` returns `true`
2. Insert root email into placeholder thread → `is_placeholder_thread` returns `false`
3. Non-placeholder thread → returns `false`
4. `list_placeholder_threads` returns correct set

**Commit**: `feat(store): add placeholder thread identification helpers`

---

### Task 4 — Thread unification (merge placeholder into real thread)

**File**: `inboxly-store/src/threading/unify.rs` (new)

When a root email arrives and a placeholder thread already exists for it, we need to unify: the arriving email joins the placeholder thread, and we clear the placeholder marker. If the arriving email was already assigned to a different thread (edge case: email arrives with its own References pointing elsewhere), we may need to merge two threads.

**Functions**:

```rust
/// Called when an email is ingested that might resolve a placeholder thread.
/// If this email's Message-ID matches a placeholder's root_message_id,
/// assign the email to that placeholder thread and clear the placeholder marker.
///
/// Returns the resolved ThreadId if unification occurred, None otherwise.
pub fn try_unify_placeholder(
    conn: &Connection,
    email_message_id: &str,
    currently_assigned_thread_id: &str,
) -> Result<Option<String>, StoreError>;

/// Merge all emails from `source_thread_id` into `target_thread_id`.
/// - UPDATE emails SET thread_id = target WHERE thread_id = source
/// - DELETE the source thread row
/// - Recalculate target thread metadata
///
/// Used when thread unification discovers two separate threads need merging.
fn merge_threads(
    conn: &Connection,
    source_thread_id: &str,
    target_thread_id: &str,
) -> Result<u64, StoreError>;  // returns number of emails moved
```

**`try_unify_placeholder` algorithm**:

```
fn try_unify_placeholder(conn, email_message_id, currently_assigned_thread_id):
    // Find any placeholder thread whose root_message_id matches this email
    placeholder_tid = SELECT id FROM threads
                      WHERE root_message_id = email_message_id
                      LIMIT 1

    if placeholder_tid is None:
        return None  // No placeholder to unify

    if placeholder_tid == currently_assigned_thread_id:
        // Email was already assigned to the placeholder thread (normal case)
        // Just clear the placeholder marker
        UPDATE threads SET root_message_id = NULL WHERE id = placeholder_tid
        return Some(placeholder_tid)

    // Edge case: email was assigned to a different thread, but a placeholder
    // exists for its Message-ID. Merge the placeholder's emails into the
    // email's current thread, then delete the placeholder.
    merge_threads(conn, placeholder_tid, currently_assigned_thread_id)
    return Some(currently_assigned_thread_id)
```

**Circular reference protection** — in `assign_thread` (Task 2), before creating a placeholder, check that the root Message-ID is not the email's own Message-ID:

```rust
// In assign_thread, before creating placeholder:
if Some(root_mid) == headers.message_id.as_deref() {
    // Self-referencing — treat as new thread
    // (some broken mailers put own Message-ID in References)
}
```

**Tests**:

1. Root email arrives, placeholder thread exists → email joins placeholder, marker cleared
2. Root email arrives, no placeholder → no-op (returns None)
3. Merge scenario: placeholder has 3 orphaned replies, root email arrives in a different thread → all 3 replies move to root's thread, placeholder deleted
4. After unification, `is_placeholder_thread` returns `false`
5. Circular reference: email References contains own Message-ID → handled without infinite loop
6. Self-referencing Message-ID in References → treated as new thread

**Commit**: `feat(store): implement thread unification for placeholder resolution`

---

### Task 5 — Thread metadata aggregation

**File**: `inboxly-store/src/threading/metadata.rs` (new)

Thread metadata must be recalculated after any change to thread membership (new email, unification, flag change). This is done via SQL aggregation queries.

**Functions**:

```rust
/// Recalculate and update the metadata for a single thread.
/// Updates: subject, newest_date, oldest_date, email_count, unread_count,
///          has_attachments, snippet.
///
/// Subject = subject of the oldest email in the thread.
/// Snippet = snippet of the newest email in the thread.
/// newest_date = MAX(date) of emails in thread.
/// oldest_date = MIN(date) of emails in thread.
/// email_count = COUNT of emails in thread.
/// unread_count = COUNT of emails where (flags & 1) = 0 (read flag not set).
/// has_attachments = MAX(has_attachments) of emails in thread.
pub fn refresh_thread_metadata(conn: &Connection, thread_id: &str) -> Result<(), StoreError>;

/// Recalculate metadata for all threads in a single account.
/// Uses a single UPDATE ... FROM (SELECT ... GROUP BY) for efficiency.
pub fn refresh_all_thread_metadata(conn: &Connection, account_id: &str) -> Result<u64, StoreError>;

/// Get aggregated thread participants (all unique from addresses).
/// Returns a JSON array of {name, address} objects for the thread.
pub fn get_thread_participants(conn: &Connection, thread_id: &str) -> Result<Vec<Contact>, StoreError>;
```

**SQL for `refresh_thread_metadata`**:

```sql
UPDATE threads SET
    subject = (SELECT subject FROM emails WHERE thread_id = ?1 ORDER BY date ASC LIMIT 1),
    newest_date = (SELECT MAX(date) FROM emails WHERE thread_id = ?1),
    oldest_date = (SELECT MIN(date) FROM emails WHERE thread_id = ?1),
    email_count = (SELECT COUNT(*) FROM emails WHERE thread_id = ?1),
    unread_count = (SELECT COUNT(*) FROM emails WHERE thread_id = ?1 AND (flags & 1) = 0),
    has_attachments = (SELECT MAX(has_attachments) FROM emails WHERE thread_id = ?1),
    snippet = (SELECT snippet FROM emails WHERE thread_id = ?1 ORDER BY date DESC LIMIT 1)
WHERE id = ?1;
```

**SQL for bulk refresh** (`refresh_all_thread_metadata`):

```sql
UPDATE threads SET
    (subject, newest_date, oldest_date, email_count, unread_count, has_attachments, snippet) = (
        SELECT
            (SELECT e2.subject FROM emails e2 WHERE e2.thread_id = threads.id ORDER BY e2.date ASC LIMIT 1),
            MAX(e.date),
            MIN(e.date),
            COUNT(*),
            SUM(CASE WHEN (e.flags & 1) = 0 THEN 1 ELSE 0 END),
            MAX(e.has_attachments),
            (SELECT e3.snippet FROM emails e3 WHERE e3.thread_id = threads.id ORDER BY e3.date DESC LIMIT 1)
        FROM emails e
        WHERE e.thread_id = threads.id
    )
WHERE threads.account_id = ?1;
```

Note: SQLite supports the tuple SET syntax only in 3.33+. If compatibility is a concern, use individual subqueries. Since the spec lists rusqlite as the dependency and CachyOS ships modern SQLite, this should be fine. Add a comment noting the version requirement.

**Tests** (in-memory SQLite):

1. Single email in thread → metadata matches that email exactly
2. Three emails in thread → newest_date, oldest_date, email_count correct
3. Mix of read/unread emails → unread_count correct
4. One email has attachment → has_attachments = true
5. Snippet is from newest email
6. Subject is from oldest email (the original message)
7. Empty thread (all emails deleted) → graceful handling (email_count = 0)
8. `refresh_all_thread_metadata` updates all threads for account
9. Participants query returns unique senders in date order

**Commit**: `feat(store): add thread metadata aggregation queries`

---

### Task 6 — Batch threading (run algorithm over all unthreaded emails)

**File**: `inboxly-store/src/threading/batch.rs` (new)

Process all emails that don't yet have a `thread_id` assigned. This is used during initial sync (M7 populates emails without threading) and for catch-up after re-threading.

**Functions**:

```rust
/// Thread all emails in the given account that have thread_id = NULL or
/// thread_id pointing to a non-existent thread.
///
/// Processes emails in date-ascending order (oldest first) so that
/// parent emails are threaded before their replies, minimizing placeholders.
///
/// Returns the number of emails threaded.
pub fn thread_unthreaded_emails(
    conn: &Connection,
    account_id: &str,
) -> Result<u64, StoreError>;

/// Thread a batch of emails by their IDs.
/// Used for incremental threading (e.g., after a sync batch).
pub fn thread_email_batch(
    conn: &Connection,
    account_id: &str,
    email_ids: &[&str],
) -> Result<u64, StoreError>;
```

**`thread_unthreaded_emails` algorithm**:

```
fn thread_unthreaded_emails(conn, account_id):
    // Fetch all unthreaded emails, oldest first
    emails = SELECT id, message_id_header, in_reply_to, references_json, subject, date
             FROM emails
             WHERE account_id = ?1 AND (thread_id IS NULL OR thread_id = '')
             ORDER BY date ASC

    // Wrap entire operation in a transaction for atomicity
    let tx = conn.transaction()?;
    let mut count = 0;

    for email in emails:
        headers = ThreadingHeaders {
            message_id: email.message_id_header,
            in_reply_to: email.in_reply_to,
            references: serde_json::from_str(email.references_json).unwrap_or_default(),
        }

        (thread_id, _) = assign_thread(&tx, account_id, &headers, &email.subject, email.date)?

        UPDATE emails SET thread_id = thread_id WHERE id = email.id

        // Check if this email resolves any placeholder
        if let Some(mid) = &headers.message_id:
            try_unify_placeholder(&tx, mid, &thread_id)?

        count += 1

    // After all assignments, refresh metadata for all touched threads
    refresh_all_thread_metadata(&tx, account_id)?

    tx.commit()?
    return count
```

**Implementation notes**:
- Use a transaction for the entire batch. For very large mailboxes (100k+), consider chunking into transactions of 5000 emails to avoid holding the write lock too long. Add a `batch_size` parameter or constant.
- The `references_json` column stores the References header as a JSON array of strings (from M3/M7 schema).
- Processing oldest-first is critical: it means parent emails get their thread assignment before child replies arrive, reducing the number of placeholder threads created.

**Tests**:

1. 10 unthreaded emails forming 3 threads → correct assignment after batch
2. Emails processed oldest-first → parent threaded before reply, no unnecessary placeholders
3. Idempotent: running twice doesn't change anything
4. Mix of standalone emails and threaded chains
5. Large batch (1000 emails) completes without error
6. `thread_email_batch` with specific IDs only threads those emails

**Commit**: `feat(store): add batch threading for unthreaded emails`

---

### Task 7 — Re-thread all emails from scratch (rebuild)

**File**: `inboxly-store/src/threading/rebuild.rs` (new)

Nuclear option: wipe all thread assignments and rebuild from scratch. Used when the threading algorithm is updated or data integrity is suspect.

**Functions**:

```rust
/// Rebuild all threads for an account from scratch.
///
/// 1. Delete all thread rows for the account
/// 2. Set all emails.thread_id = NULL for the account
/// 3. Run thread_unthreaded_emails to reassign everything
///
/// This is a destructive operation on thread_state (pins, snooze, bundle assignments
/// are keyed on thread_id). Callers should warn the user that pins/snooze will be lost.
///
/// Returns the number of emails re-threaded.
pub fn rebuild_threads(conn: &Connection, account_id: &str) -> Result<u64, StoreError>;
```

**Algorithm**:

```
fn rebuild_threads(conn, account_id):
    let tx = conn.transaction()?;

    // 1. Clear thread assignments
    UPDATE emails SET thread_id = NULL WHERE account_id = ?1

    // 2. Delete thread_state rows for threads being deleted
    DELETE FROM thread_state WHERE thread_id IN (
        SELECT id FROM threads WHERE account_id = ?1
    )

    // 3. Delete all thread rows
    DELETE FROM threads WHERE account_id = ?1

    tx.commit()?;

    // 4. Rebuild (outside transaction — thread_unthreaded_emails makes its own)
    thread_unthreaded_emails(conn, account_id)
```

**Tests**:

1. Rebuild on account with existing threads → same threads recreated (deterministic)
2. Rebuild produces identical thread assignments as original batch threading (verify thread membership, not thread IDs which are random UUIDs)
3. After rebuild, thread metadata is correct
4. Empty account → no-op, returns 0
5. Thread_state rows are deleted (pins/snooze cleared)

**Commit**: `feat(store): add full thread rebuild capability`

---

### Task 8 — Hook into email ingest pipeline (auto-thread on insert)

**File**: `inboxly-store/src/store.rs` (modify existing — or wherever M3/M7 placed the email insert function)

Integrate the threading algorithm into the email insertion path so that every email is automatically threaded on ingest.

**Changes to existing code**:

Find the existing `insert_email` (or equivalent) function from M3. Modify it to:

1. Extract threading headers from the email's header map
2. Call `assign_thread` to get a `ThreadId`
3. Set the `thread_id` on the email row
4. Call `try_unify_placeholder` in case this email resolves a placeholder
5. Call `refresh_thread_metadata` for the affected thread

```rust
// In the existing insert_email function, after inserting the email row:

// --- Threading integration ---
let headers = extract_threading_headers(&email.headers);

let (thread_id, _new) = assign_thread(
    conn,
    &email.account_id,
    &headers,
    &email.subject,
    email.date,
)?;

// Update the email's thread_id
conn.execute(
    "UPDATE emails SET thread_id = ?1 WHERE id = ?2",
    params![thread_id, email.id],
)?;

// Check if this email resolves a placeholder thread
if let Some(mid) = &headers.message_id {
    if let Some(resolved_tid) = try_unify_placeholder(conn, mid, &thread_id)? {
        // If unification changed the thread, update the email's thread_id
        if resolved_tid != thread_id {
            conn.execute(
                "UPDATE emails SET thread_id = ?1 WHERE id = ?2",
                params![resolved_tid, email.id],
            )?;
        }
        refresh_thread_metadata(conn, &resolved_tid)?;
    }
}

refresh_thread_metadata(conn, &thread_id)?;
// --- End threading integration ---
```

**Also modify**: If M7's sync writes emails in batches, ensure the threading hook runs within the same transaction. If M7 uses a bulk INSERT, add a post-batch call to `thread_email_batch` instead of per-email threading. The per-email approach is cleaner but the batch approach is faster for initial sync.

**Decision**: Implement both paths:
- `insert_email_with_threading(conn, email)` — single email, used for incremental sync / IDLE push
- After bulk insert (initial sync), call `thread_email_batch(conn, account_id, &new_email_ids)`

**Tests**:

1. Insert email with References → automatically threaded
2. Insert reply before parent → placeholder created, reply threaded to placeholder
3. Insert parent after reply → placeholder resolved, both emails in same thread
4. Insert standalone email → new thread created
5. Thread metadata up-to-date after each insert
6. Bulk insert of 50 emails → all correctly threaded after batch call

**Commit**: `feat(store): integrate threading into email ingest pipeline`

---

### Task 9 — Edge case tests

**File**: `inboxly-store/src/threading/tests/edge_cases.rs` (new)

Comprehensive test module for unusual and broken inputs.

**Test cases**:

```rust
#[test]
fn test_no_headers_at_all() {
    // Email with completely empty headers map
    // → new standalone thread
}

#[test]
fn test_empty_message_id() {
    // Message-ID header present but value is empty string
    // → treated as no Message-ID
}

#[test]
fn test_empty_references() {
    // References header present but value is empty string
    // → treated as no References
}

#[test]
fn test_references_with_only_whitespace() {
    // References: "   \t  \n  "
    // → treated as no References
}

#[test]
fn test_broken_angle_brackets() {
    // References: "<abc@example.com <def@example.com>"
    // → best-effort parse, extract what we can
}

#[test]
fn test_missing_closing_bracket() {
    // References: "<abc@example.com"
    // → extract "abc@example.com"
}

#[test]
fn test_no_angle_brackets() {
    // References: "abc@example.com def@example.com"
    // → split on whitespace, treat each as Message-ID
}

#[test]
fn test_circular_references_self() {
    // Message-ID: <A>, References: <A>
    // → email references itself, should create standalone thread
}

#[test]
fn test_circular_references_mutual() {
    // Email 1: Message-ID: <A>, References: <B>
    // Email 2: Message-ID: <B>, References: <A>
    // → both should end up in the same thread, no infinite loop
}

#[test]
fn test_very_long_references_chain() {
    // References header with 200+ Message-IDs
    // → only first (root) matters, should handle efficiently
}

#[test]
fn test_duplicate_message_id() {
    // Two emails with the same Message-ID (happens with resent/bounced mail)
    // → second email should join first email's thread
}

#[test]
fn test_references_root_not_first() {
    // Verify that we always use references[0] as root, not references[last]
    // (Some implementations use last, but spec says first = original)
}

#[test]
fn test_unicode_in_message_id() {
    // Message-ID containing non-ASCII characters
    // → preserve as-is, don't reject
}

#[test]
fn test_very_long_message_id() {
    // Message-ID with 500+ characters (spam/broken mailers)
    // → handle without truncation error
}

#[test]
fn test_thread_with_single_email_deleted() {
    // Thread has 1 email, email is removed
    // → thread metadata shows 0 emails, thread is empty but still exists
}

#[test]
fn test_in_reply_to_with_multiple_ids() {
    // In-Reply-To: <a@ex.com> <b@ex.com>
    // → take only the first one
}

#[test]
fn test_references_overrides_in_reply_to() {
    // References points to thread A, In-Reply-To points to thread B
    // → email joins thread A (References takes precedence)
}
```

**Commit**: `test(store): add threading edge case tests`

---

### Task 10 — Integration tests with realistic email chains

**File**: `inboxly-store/tests/threading_integration.rs` (new, integration test)

End-to-end tests using realistic email scenarios with the full store API.

**Test fixtures** — create a helper module with factory functions:

```rust
/// Create a mock email with the given threading headers.
fn make_email(
    id: &str,
    message_id: &str,
    in_reply_to: Option<&str>,
    references: &[&str],
    subject: &str,
    date: i64,
) -> EmailMeta { ... }
```

**Test scenarios**:

```rust
#[test]
fn test_simple_two_email_thread() {
    // 1. Insert original: Message-ID: <orig>, no References
    // 2. Insert reply: Message-ID: <reply>, In-Reply-To: <orig>, References: <orig>
    // → both in same thread, thread has 2 emails, subject from original
}

#[test]
fn test_three_level_thread() {
    // orig → reply1 → reply2
    // All share References[0] = <orig>
    // → single thread with 3 emails
}

#[test]
fn test_branching_thread() {
    // orig → reply_a (branch 1)
    //      → reply_b (branch 2)
    // Both replies have References: <orig>
    // → all 3 in same thread
}

#[test]
fn test_reply_before_parent() {
    // 1. Insert reply: References: <orig>
    //    → placeholder thread created
    // 2. Insert orig: Message-ID: <orig>, no References
    //    → placeholder resolved, both in same thread
    // 3. Verify thread metadata: subject from orig (oldest), snippet from reply (newest)
}

#[test]
fn test_deep_reply_chain_out_of_order() {
    // Insert emails 5, 3, 1, 4, 2 (out of date order)
    // All have References: <email1> ... (growing chain)
    // → all end up in one thread
    // → after metadata refresh, dates correct
}

#[test]
fn test_multiple_independent_threads() {
    // Insert 5 unrelated emails (no threading headers)
    // → 5 separate threads
}

#[test]
fn test_mixed_threads_and_standalone() {
    // 3-email thread + 2 standalone emails
    // → 3 threads total (1 with 3 emails, 2 with 1 each)
}

#[test]
fn test_gmail_style_references() {
    // Gmail includes full References chain:
    // email1: Message-ID: <a>, References: (none)
    // email2: Message-ID: <b>, References: <a>, In-Reply-To: <a>
    // email3: Message-ID: <c>, References: <a> <b>, In-Reply-To: <b>
    // email4: Message-ID: <d>, References: <a> <b> <c>, In-Reply-To: <c>
    // → all in one thread (root = <a>)
}

#[test]
fn test_cross_post_different_references() {
    // Two replies to same email but from different mailing lists
    // One has References: <root> <a>, other has References: <root> <b>
    // → both in same thread (same root)
}

#[test]
fn test_rebuild_preserves_thread_membership() {
    // 1. Insert 20 emails forming 5 threads
    // 2. Record which emails are in which threads
    // 3. Call rebuild_threads
    // 4. Verify same emails grouped together (thread IDs will differ)
}

#[test]
fn test_thread_metadata_after_flag_change() {
    // 1. Insert 3-email thread, all unread
    // 2. Mark one as read
    // 3. Refresh metadata
    // 4. Verify unread_count = 2
}

#[test]
fn test_batch_threading_performance() {
    // Insert 1000 emails (500 in chains, 500 standalone)
    // Run thread_unthreaded_emails
    // Verify completes in < 1 second
    // Verify correct thread count
}

#[test]
fn test_concurrent_account_threading() {
    // Two accounts with independent emails
    // Thread both accounts
    // Verify no cross-account thread contamination
}
```

**Commit**: `test(store): add threading integration tests with realistic email chains`

---

## Module Structure

After all tasks, the threading module layout:

```
inboxly-store/src/
├── threading/
│   ├── mod.rs          ← re-exports public API
│   ├── headers.rs      ← Task 1: header extraction
│   ├── assign.rs       ← Task 2+3: thread assignment + placeholder tracking
│   ├── unify.rs        ← Task 4: thread unification
│   ├── metadata.rs     ← Task 5: thread metadata aggregation
│   ├── batch.rs        ← Task 6: batch threading
│   ├── rebuild.rs      ← Task 7: re-thread from scratch
│   └── tests/
│       ├── mod.rs
│       └── edge_cases.rs  ← Task 9
├── store.rs            ← Task 8: modified to hook threading into ingest
└── ...
```

**Public API** (re-exported from `threading/mod.rs`):

```rust
pub mod threading;

// Public types
pub use threading::headers::ThreadingHeaders;

// Public functions
pub use threading::headers::extract_threading_headers;
pub use threading::assign::assign_thread;
pub use threading::unify::try_unify_placeholder;
pub use threading::metadata::{refresh_thread_metadata, refresh_all_thread_metadata, get_thread_participants};
pub use threading::batch::{thread_unthreaded_emails, thread_email_batch};
pub use threading::rebuild::rebuild_threads;
pub use threading::assign::{is_placeholder_thread, list_placeholder_threads};
```

---

## Schema Changes Summary

One new column on the `threads` table:

```sql
ALTER TABLE threads ADD COLUMN root_message_id TEXT;
CREATE INDEX idx_threads_root_message_id ON threads(root_message_id);
```

No changes to the `emails` table — it already has `message_id_header`, `in_reply_to`, `references_json`, and `thread_id` columns from M3.

---

## Build & Verify

```bash
# From workspace root
cargo test -p inboxly-store                    # all store tests including threading
cargo test -p inboxly-store -- threading       # just threading tests
cargo clippy -p inboxly-store -- -D warnings   # lint clean
```

---

## Commit Sequence

| # | Message |
|---|---------|
| 1 | `feat(store): add threading header extraction utility` |
| 2 | `feat(store): implement core thread assignment algorithm` |
| 3 | `feat(store): add placeholder thread identification helpers` |
| 4 | `feat(store): implement thread unification for placeholder resolution` |
| 5 | `feat(store): add thread metadata aggregation queries` |
| 6 | `feat(store): add batch threading for unthreaded emails` |
| 7 | `feat(store): add full thread rebuild capability` |
| 8 | `feat(store): integrate threading into email ingest pipeline` |
| 9 | `test(store): add threading edge case tests` |
| 10 | `test(store): add threading integration tests with realistic email chains` |
