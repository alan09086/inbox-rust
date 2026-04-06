# M34: Thread Detail View + HTML Email Rendering

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` to implement this plan task-by-task with two-stage review (spec compliance + code quality) after each phase.

**Goal:** Let the user click an email row to read the full thread — sender, date, sanitised HTML body, and a list of messages stacked vertically. Read-only; reply/forward come in M36.

**Architecture:** A new Dioxus component `ThreadDetailView` renders from a `LoadedThread` data structure built by a synchronous loader function. The loader reads the per-thread `EmailRow` list from `Store::get_emails_by_thread()`, then for each row reads the full `EmailContent` from disk via `MaildirStore::read_email_content(maildir_path)`. HTML bodies are sanitised with `ammonia` (a Rust HTML sanitiser) before being injected via Dioxus's `dangerous_inner_html` attribute. A "demo loader" produces fake `LoadedThread` values so we can verify rendering before real IMAP sync exists.

**Tech Stack:** Dioxus 0.7.4 desktop, `ammonia` 4.x for HTML sanitisation, `chrono` (already a workspace dep) for date formatting, the existing `inboxly-store::Store` and `inboxly-store::maildir_store::MaildirStore` types.

**Source spec:** `docs/superpowers/specs/2026-04-06-inboxly-v2-full-client-design.md` §M32 (renumbered to M34 in `docs/superpowers/plans/roadmap.md` to honour the framework-conversion detour).

---

## Context

After M33, the inbox feed renders email rows but clicking one does nothing. The full body content exists in the `EmailContent` struct (already defined in `inboxly-core/src/email.rs:77`) and is loaded from disk by `MaildirStore::read_email_content()` (already implemented in `inboxly-store/src/maildir_store.rs:578`). The plumbing is in place. M34 wires the UI side so the user can click an email row, see all messages in the thread stacked vertically with their sanitised HTML bodies, and dismiss back to the inbox via a back arrow or Escape key.

**What exists:**
- `EmailRow` struct with `thread_id`, `from_name`, `from_address`, `subject`, `date`, `maildir_path`, `body_downloaded`, etc. (`inboxly-store/src/emails.rs:17`)
- `Store::get_emails_by_thread(thread_id) -> Result<Vec<EmailRow>>` (`inboxly-store/src/emails.rs:99`)
- `EmailContent { id, body_text, body_html, headers, attachments }` (`inboxly-core/src/email.rs:77`)
- `MaildirStore::read_email_content(&Path) -> Result<EmailContent>` (`inboxly-store/src/maildir_store.rs:578`)
- Dioxus `dangerous_inner_html` attribute (verified in `dioxus-html-0.7.4/src/attribute_groups.rs:356`)
- The `ammonia` crate is the standard Rust HTML sanitiser (whitelist-based, strips JS, used by hundreds of crates)

**What's missing:**
- `Inboxly` has no field tracking the currently-open thread or its loaded content
- No `Message::OpenThread` / `Message::CloseThread` variants
- No `MaildirStore` reference inside `Inboxly` (the UI currently only knows about `Store`)
- No `ammonia` dependency
- No `ThreadDetailView` Dioxus component
- `EmailRow` has no row-level click handler — only `oncontextmenu` and the overflow button's `onclick`
- No CSS for `.thread-detail-view` or its child classes
- No demo loader producing fake `LoadedThread` values for testing

---

## File Structure

| File | Type | Purpose |
|------|------|---------|
| `inboxly-ui/Cargo.toml` | modify | Add `ammonia.workspace = true` |
| `Cargo.toml` (workspace) | modify | Add `ammonia = "4"` to `[workspace.dependencies]` |
| `inboxly-ui/src/app.rs` | modify | Add `open_thread: Option<LoadedThread>` field, `Maildir` field, `OpenThread`/`CloseThread` messages and handlers |
| `inboxly-ui/src/loaded_thread.rs` | create | `LoadedThread` struct + `LoadedMessage` struct + `load_thread()` function + `demo_thread()` helper |
| `inboxly-ui/src/sanitize.rs` | create | `sanitize_html(raw: &str) -> String` wrapper around ammonia with project's whitelist |
| `inboxly-ui/src/components/thread_detail_view.rs` | create | The Dioxus component — renders header bar, message list, per-message body |
| `inboxly-ui/src/components/thread_message.rs` | create | A child component rendering one message: avatar, sender, date, body |
| `inboxly-ui/src/components/email_row.rs` | modify | Add row-level `onclick` that dispatches `Message::OpenThread(thread_id)` |
| `inboxly-ui/src/components/content_area.rs` | modify | When `open_thread.is_some()`, render `ThreadDetailView` instead of `InboxFeed` (only inside the Inbox view branch) |
| `inboxly-ui/src/components/mod.rs` | modify | Register `thread_detail_view` and `thread_message` modules |
| `inboxly-ui/assets/main.css` | modify | Add `.thread-detail-view`, `.thread-detail-header`, `.thread-message`, `.thread-message-header`, `.thread-message-body`, `.thread-detail-empty`, `.thread-message-attachments` |
| `inboxly-ui/src/keyboard.rs` | modify | If a thread is open, Escape dispatches `Message::CloseThread` |

**11 files (3 created, 8 modified, 0 deleted).** The two new components keep file responsibilities focused: `thread_detail_view.rs` handles the layout shell and header; `thread_message.rs` handles a single message's rendering. This split mirrors `inbox_feed.rs` + `email_row.rs` from M33.

---

## State Changes

Add to `Inboxly` struct in `inboxly-ui/src/app.rs` (place near `feed_sections`):

```rust
/// Currently-open thread, if any. When `Some`, the ContentArea
/// renders ThreadDetailView instead of InboxFeed (Inbox view only).
pub open_thread: Option<crate::loaded_thread::LoadedThread>,

/// Optional Maildir handle for loading email bodies. None when no
/// account is configured. When None, opening a thread shows demo
/// content via `loaded_thread::demo_thread()`.
pub maildir: Option<std::sync::Arc<inboxly_store::maildir_store::MaildirStore>>,
```

Initialize in `Default`:
- `open_thread: None`
- `maildir: None`

Add `Message` variants:

```rust
/// Open the full thread detail view for a thread ID.
OpenThread(String),
/// Close the open thread and return to the inbox feed.
CloseThread,
```

Handlers in `update()`:

```rust
Message::OpenThread(thread_id) => {
    // Try to load the real thread; fall back to demo content
    // when no store data is available (M34 ships without sync wired).
    let loaded = match (self.store.as_ref(), self.maildir.as_ref()) {
        (Some(store), Some(maildir)) => {
            crate::loaded_thread::load_thread(store, maildir, &thread_id)
                .unwrap_or_else(|_| crate::loaded_thread::demo_thread(&thread_id))
        }
        _ => crate::loaded_thread::demo_thread(&thread_id),
    };
    self.open_thread = Some(loaded);
    self.close_menus();
}
Message::CloseThread => {
    self.open_thread = None;
}
```

The `close_menus()` call is intentional — opening a thread should dismiss any open context/overflow menu.

---

## Design Decisions

- **`LoadedThread` is a UI-owned data structure**, not a `Store` row. It bundles the per-thread metadata + a Vec of `LoadedMessage` (each carrying the parsed `EmailContent`). Decoupling from the store rows keeps the component pure (no DB access during render).
- **Synchronous loader for now.** `load_thread()` does blocking file I/O when called, but at M33's data scale (hundreds of messages per thread max) this is fine. Async loading via `use_resource` is future work — note in code but don't implement.
- **HTML sanitisation via `ammonia` with the default whitelist.** Ammonia's defaults strip `<script>`, `<style>`, `javascript:` URLs, event handlers, `<iframe>`, and most other unsafe constructs. Output is safe to inject via `dangerous_inner_html`. We do NOT customise the whitelist in M34 — defaults are sufficient for an MVP.
- **Plain-text fallback.** If `body_html` is `None` but `body_text` is `Some`, render the text inside a `<pre>` tag wrapped in our own escape (no ammonia needed for already-text content). If both are `None`, render `"(no content)"`.
- **No link interception in M34.** Clicking a link inside an email body navigates the WebKitGTK webview to the URL. This is unsafe (it replaces the app contents). Mitigation: ammonia strips `javascript:` URLs by default, so the worst case is replacing the app with the linked page. Real link interception (open in system browser) requires Dioxus webview navigation policy hooks and is a Phase 7 task.
- **No quoted-content collapse in M34.** The v2 spec mentions "show trimmed content" expander; that's a polish item we defer. M34 renders the full body verbatim.
- **Attachment list shows metadata only.** `LoadedMessage.attachments` is `Vec<AttachmentMeta>` (from `EmailContent.attachments` mapped to drop the byte content). Click handlers are no-ops (download is M37). Just a list of "filename — MIME — size".
- **`open_thread` lives on `Inboxly`, not on the `ThreadDetailView` component.** This means the back button dispatches a message rather than a local state mutation, keeping ThreadDetailView a stateless renderer of the loaded data.
- **Demo loader.** `demo_thread(thread_id)` produces a `LoadedThread` with two fake messages (one HTML, one plain-text), several headers, and one fake attachment. Used when `Inboxly::store` or `Inboxly::maildir` is None. Lets us visually verify the rendering without wiring real sync.
- **Escape key is handled in `keyboard.rs`** rather than directly on the ThreadDetailView component, matching the existing keyboard shortcut routing pattern.
- **`Inboxly::store: Option<...>`** — wait, the current `Inboxly` doesn't have a `store` field at all? Verify before Phase 1: if there's no `store: Option<Store>` field, the loader path simplifies to "always use demo data in M34, store-backed loading deferred to the milestone that wires real sync." Adapt the handlers accordingly. The Phase 1 task explicitly checks this.

---

## New Components (3 files)

| File | Purpose | Estimated LOC |
|------|---------|--------------|
| `loaded_thread.rs` | Pure data: `LoadedThread`, `LoadedMessage`, `load_thread()`, `demo_thread()`, unit tests for `demo_thread()` | ~150 |
| `sanitize.rs` | `sanitize_html()` wrapper, unit tests for "strips script tags", "strips javascript: URLs", "preserves text" | ~50 |
| `components/thread_detail_view.rs` | Layout shell: header bar with back button + subject, list of `ThreadMessage` children, empty state | ~80 |
| `components/thread_message.rs` | Single message: avatar, sender name + email, formatted date, body div, attachment list | ~120 |

---

## Implementation Order

### Phase 1: Verify `Inboxly` shape and add foundational state

- [ ] **Step 1.1: Confirm the existing `Inboxly::store` field shape**

  Run: `grep -n "^    pub store\|^    store:" inboxly-ui/src/app.rs`

  Expected: `pub store: Option<Store>` at around line 105 (verified during plan writing). The `Option` matters because the UI runs without a configured account in M34 — use `self.store.as_ref()` to access it. If the type has changed since the plan was written (e.g. someone wrapped it in `Arc`), adapt the borrows in Phase 4 accordingly.

- [ ] **Step 1.2: Add `ammonia` to workspace dependencies**

  Edit `Cargo.toml` (workspace root) `[workspace.dependencies]` section, add:
  ```toml
  ammonia = "4"
  ```
  Edit `inboxly-ui/Cargo.toml` `[dependencies]` section, add:
  ```toml
  ammonia.workspace = true
  ```

- [ ] **Step 1.3: Add `open_thread` and `maildir` fields to `Inboxly`**

  In `inboxly-ui/src/app.rs`, locate the `Inboxly` struct. Add (place near `feed_sections`):
  ```rust
  pub open_thread: Option<crate::loaded_thread::LoadedThread>,
  pub maildir: Option<std::sync::Arc<inboxly_store::maildir_store::MaildirStore>>,
  ```

  Initialize both to `None` in `Default::default()`.

  At this point `loaded_thread` doesn't exist yet, so the build will fail. That's expected — Phase 2 creates the module.

- [ ] **Step 1.4: Add `OpenThread` and `CloseThread` Message variants**

  In `inboxly-ui/src/app.rs`, locate the `Message` enum. Add (place near `OpenContextMenu`):
  ```rust
  /// Open the full thread detail view for a thread ID.
  OpenThread(String),
  /// Close the open thread and return to the inbox feed.
  CloseThread,
  ```

- [ ] **Step 1.5: Add Message handlers (placeholder bodies)**

  In `update()`, add the two new arms with placeholder bodies:
  ```rust
  Message::OpenThread(thread_id) => {
      self.open_thread = Some(crate::loaded_thread::demo_thread(&thread_id));
      self.close_menus();
  }
  Message::CloseThread => {
      self.open_thread = None;
  }
  ```

  This intentionally always uses the demo loader for now — Phase 4 wires real loading once `LoadedThread` exists.

- [ ] **Step 1.6: Build (will fail) and confirm the failure is the expected unresolved-import**

  Run: `cargo build -p inboxly-ui 2>&1 | tail -5`

  Expected: error `unresolved module crate::loaded_thread` and possibly `unresolved import inboxly_store::maildir_store`. Confirm the failure is what you expected before moving on.

- [ ] **Step 1.7: Commit**

  ```bash
  git add Cargo.toml inboxly-ui/Cargo.toml inboxly-ui/src/app.rs
  git commit -m "feat(ui): add OpenThread/CloseThread state and ammonia dep (M34 phase 1)"
  ```

### Phase 2: `LoadedThread` data structure + `demo_thread()`

- [ ] **Step 2.1: Write the failing test for `demo_thread()`**

  Create `inboxly-ui/src/loaded_thread.rs` with just the test stub:
  ```rust
  //! Loaded thread data structure for the thread detail view.

  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn demo_thread_has_two_messages() {
          let thread = demo_thread("demo-thread-1");
          assert_eq!(thread.thread_id, "demo-thread-1");
          assert_eq!(thread.messages.len(), 2);
          assert_eq!(thread.subject, "Welcome to Inboxly");
      }

      #[test]
      fn demo_thread_first_message_has_html_body() {
          let thread = demo_thread("demo");
          let first = &thread.messages[0];
          assert!(first.body_html.is_some());
          assert!(first.body_html.as_ref().unwrap().contains("<p>"));
      }

      #[test]
      fn demo_thread_second_message_is_plain_text() {
          let thread = demo_thread("demo");
          let second = &thread.messages[1];
          assert!(second.body_html.is_none());
          assert!(second.body_text.is_some());
      }
  }
  ```

  Add `pub mod loaded_thread;` to `inboxly-ui/src/lib.rs`.

  Run: `cargo test -p inboxly-ui loaded_thread 2>&1 | tail -10`
  Expected: compile error — `LoadedThread`, `LoadedMessage`, `demo_thread` not defined.

- [ ] **Step 2.2: Implement `LoadedThread`, `LoadedMessage`, and `demo_thread()`**

  Replace the test-only file with the full module. Place above the `#[cfg(test)] mod tests { ... }` block:
  ```rust
  //! Loaded thread data structure for the thread detail view.
  //!
  //! `LoadedThread` is a UI-owned bundle of the per-thread metadata
  //! plus all messages with their full content (body, headers, attachments).
  //! Built by `load_thread()` from a `Store` + `MaildirStore` pair, OR by
  //! `demo_thread()` for testing/demo when no real store data is available.

  use chrono::{DateTime, Utc};

  use inboxly_core::AttachmentMeta;

  /// All data needed to render the thread detail view.
  #[derive(Debug, Clone, PartialEq)]
  pub struct LoadedThread {
      pub thread_id: String,
      pub subject: String,
      pub messages: Vec<LoadedMessage>,
  }

  /// One message inside a loaded thread.
  #[derive(Debug, Clone, PartialEq)]
  pub struct LoadedMessage {
      pub email_id: String,
      pub from_name: String,
      pub from_address: String,
      pub date: DateTime<Utc>,
      pub body_text: Option<String>,
      pub body_html: Option<String>,
      pub attachments: Vec<AttachmentMeta>,
  }

  /// Build a fake thread with two messages — used when no real store
  /// data is available, so the rendering pipeline can be exercised.
  pub fn demo_thread(thread_id: &str) -> LoadedThread {
      LoadedThread {
          thread_id: thread_id.to_string(),
          subject: "Welcome to Inboxly".to_string(),
          messages: vec![
              LoadedMessage {
                  email_id: format!("{thread_id}-1"),
                  from_name: "Alan Gaudet".to_string(),
                  from_address: "alan@example.com".to_string(),
                  date: chrono::Utc::now() - chrono::Duration::hours(2),
                  body_html: Some(
                      "<p>Hi there,</p><p>This is a <strong>demo</strong> message rendered \
                       from the M34 thread detail view. The body is sanitised HTML.</p>\
                       <p>Cheers,<br>Alan</p>"
                          .to_string(),
                  ),
                  body_text: None,
                  attachments: vec![AttachmentMeta {
                      filename: "report.pdf".to_string(),
                      mime_type: "application/pdf".to_string(),
                      size_bytes: 124_532,
                  }],
              },
              LoadedMessage {
                  email_id: format!("{thread_id}-2"),
                  from_name: "Test Sender".to_string(),
                  from_address: "test@example.com".to_string(),
                  date: chrono::Utc::now() - chrono::Duration::minutes(15),
                  body_html: None,
                  body_text: Some(
                      "Reply with a plain-text body.\n\nNo HTML, no formatting.\n\nLine three."
                          .to_string(),
                  ),
                  attachments: vec![],
              },
          ],
      }
  }
  ```

  Run: `cargo test -p inboxly-ui loaded_thread 2>&1 | tail -10`
  Expected: 3 tests pass.

- [ ] **Step 2.3: Build the whole UI crate**

  Run: `cargo build -p inboxly-ui 2>&1 | tail -5`
  Expected: clean — Phase 1's `OpenThread` handler that referenced `crate::loaded_thread::demo_thread` should now resolve.

- [ ] **Step 2.4: Commit**

  ```bash
  git add inboxly-ui/src/loaded_thread.rs inboxly-ui/src/lib.rs
  git commit -m "feat(ui): add LoadedThread data structure and demo_thread fixture (M34 phase 2)"
  ```

### Phase 3: HTML sanitisation helper

- [ ] **Step 3.1: Write the failing tests**

  Create `inboxly-ui/src/sanitize.rs`:
  ```rust
  //! HTML sanitisation for email body rendering.
  //!
  //! Email HTML is untrusted — it may come from any sender and may
  //! contain `<script>`, event handlers, `javascript:` URLs, or other
  //! XSS vectors. This module wraps `ammonia` with the project's
  //! settings (currently the defaults) to produce HTML safe to inject
  //! via Dioxus's `dangerous_inner_html`.

  /// Sanitise an HTML email body for safe rendering.
  ///
  /// Strips scripts, event handlers, `javascript:` URLs, iframes, and
  /// other unsafe constructs via `ammonia`'s default whitelist.
  pub fn sanitize_html(raw: &str) -> String {
      ammonia::clean(raw)
  }

  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn strips_script_tags() {
          let dirty = "<p>Hi</p><script>alert('xss')</script><p>bye</p>";
          let clean = sanitize_html(dirty);
          assert!(!clean.contains("<script"));
          assert!(!clean.contains("alert"));
          assert!(clean.contains("<p>Hi</p>"));
          assert!(clean.contains("<p>bye</p>"));
      }

      #[test]
      fn strips_javascript_urls() {
          let dirty = "<a href=\"javascript:alert(1)\">click</a>";
          let clean = sanitize_html(dirty);
          assert!(!clean.contains("javascript:"));
      }

      #[test]
      fn strips_event_handlers() {
          let dirty = "<img src=\"x\" onerror=\"alert(1)\">";
          let clean = sanitize_html(dirty);
          assert!(!clean.contains("onerror"));
      }

      #[test]
      fn preserves_safe_formatting() {
          let dirty = "<p><strong>bold</strong> and <em>italic</em></p>";
          let clean = sanitize_html(dirty);
          assert!(clean.contains("<strong>bold</strong>"));
          assert!(clean.contains("<em>italic</em>"));
      }

      #[test]
      fn preserves_anchor_with_safe_href() {
          let dirty = "<a href=\"https://example.com\">link</a>";
          let clean = sanitize_html(dirty);
          assert!(clean.contains("href=\"https://example.com\""));
          assert!(clean.contains(">link</a>"));
      }
  }
  ```

  Add `pub mod sanitize;` to `inboxly-ui/src/lib.rs`.

  Run: `cargo test -p inboxly-ui sanitize 2>&1 | tail -10`
  Expected: 5 tests pass (ammonia handles all of these by default).

- [ ] **Step 3.2: Commit**

  ```bash
  git add inboxly-ui/src/sanitize.rs inboxly-ui/src/lib.rs
  git commit -m "feat(ui): add HTML sanitisation helper via ammonia (M34 phase 3)"
  ```

### Phase 4: Real-store loader (defensive — only if `Inboxly::store` exists)

- [ ] **Step 4.1: Add `load_thread()` function to `loaded_thread.rs`**

  Append to `inboxly-ui/src/loaded_thread.rs` (above the `#[cfg(test)]` block):
  ```rust
  use std::path::Path;

  use inboxly_store::maildir_store::MaildirStore;
  use inboxly_store::store::Store;

  /// Load a thread from the real store + maildir, returning a `LoadedThread`
  /// suitable for the thread detail view.
  ///
  /// Errors if the thread has no emails or if any email's body cannot be
  /// read from disk. Callers should fall back to `demo_thread()` on error.
  pub fn load_thread(
      store: &Store,
      maildir: &MaildirStore,
      thread_id: &str,
  ) -> Result<LoadedThread, String> {
      let rows = store
          .get_emails_by_thread(thread_id)
          .map_err(|e| format!("get_emails_by_thread failed: {e}"))?;
      if rows.is_empty() {
          return Err(format!("no emails in thread {thread_id}"));
      }

      let subject = rows[0].subject.clone();
      let mut messages = Vec::with_capacity(rows.len());
      for row in &rows {
          let body_text;
          let body_html;
          let attachments;
          if row.body_downloaded && !row.maildir_path.is_empty() {
              match maildir.read_email_content(Path::new(&row.maildir_path)) {
                  Ok(content) => {
                      body_text = content.body_text;
                      body_html = content.body_html;
                      attachments = content
                          .attachments
                          .into_iter()
                          .map(|a| a.meta)
                          .collect();
                  }
                  Err(_) => {
                      // Fall back to placeholder if read fails.
                      body_text = Some("(failed to read body from disk)".to_string());
                      body_html = None;
                      attachments = Vec::new();
                  }
              }
          } else {
              body_text = Some("(body not yet downloaded)".to_string());
              body_html = None;
              attachments = Vec::new();
          }

          messages.push(LoadedMessage {
              email_id: row.id.clone(),
              from_name: row.from_name.clone().unwrap_or_else(|| row.from_address.clone()),
              from_address: row.from_address.clone(),
              date: chrono::DateTime::<chrono::Utc>::from_timestamp(row.date, 0)
                  .unwrap_or_else(|| chrono::Utc::now()),
              body_text,
              body_html,
              attachments,
          });
      }

      Ok(LoadedThread {
          thread_id: thread_id.to_string(),
          subject,
          messages,
      })
  }
  ```

- [ ] **Step 4.2: Update the `OpenThread` handler to try the real loader**

  In `inboxly-ui/src/app.rs`, replace the placeholder `OpenThread` handler with:
  ```rust
  Message::OpenThread(thread_id) => {
      let loaded = match (self.store.as_ref(), self.maildir.as_ref()) {
          (Some(store), Some(maildir)) => {
              crate::loaded_thread::load_thread(store, maildir, &thread_id)
                  .unwrap_or_else(|_| crate::loaded_thread::demo_thread(&thread_id))
          }
          _ => crate::loaded_thread::demo_thread(&thread_id),
      };
      self.open_thread = Some(loaded);
      self.close_menus();
  }
  ```

- [ ] **Step 4.3: Run tests**

  Run: `cargo test -p inboxly-ui 2>&1 | grep "test result" | head -3`
  Expected: all prior tests still pass; the new loader is exercised by the existing demo_thread tests indirectly (it shares the LoadedThread/LoadedMessage types).

- [ ] **Step 4.4: Commit**

  ```bash
  git add inboxly-ui/src/loaded_thread.rs inboxly-ui/src/app.rs
  git commit -m "feat(ui): add real-store thread loader with demo fallback (M34 phase 4)"
  ```

### Phase 5: CSS foundation for thread detail view

- [ ] **Step 5.1: Add new CSS classes to `inboxly-ui/assets/main.css`**

  Append at the end of the file (after the empty-states section). The classes use existing custom properties so dark/light mode is automatic.

  ```css
  /* ============================================================
     15. Thread detail view (M34)
     ============================================================ */

  .thread-detail-view {
      display: flex;
      flex-direction: column;
      flex: 1;
      overflow-y: auto;
      background: var(--bg-color);
  }

  .thread-detail-header {
      display: flex;
      align-items: center;
      gap: 16px;
      padding: 16px var(--default-padding);
      background: var(--surface-color);
      border-bottom: var(--divider-thickness) solid var(--divider-color);
      position: sticky;
      top: 0;
      z-index: 10;
  }

  .thread-detail-back {
      background: transparent;
      border: none;
      color: var(--text-primary);
      font-size: 22px;
      cursor: pointer;
      padding: 6px 10px;
      border-radius: 50%;
  }

  .thread-detail-back:hover {
      background: var(--menu-hover);
  }

  .thread-detail-subject {
      font-size: 18px;
      font-weight: 500;
      color: var(--text-primary);
      flex: 1;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
  }

  .thread-message {
      background: var(--surface-color);
      margin: 12px var(--default-padding);
      border-radius: 8px;
      box-shadow: 0 1px 2px var(--menu-shadow);
      overflow: hidden;
  }

  .thread-message-header {
      display: flex;
      align-items: center;
      gap: 12px;
      padding: 14px 18px;
      border-bottom: var(--divider-thickness) solid var(--divider-color);
  }

  .thread-message-from {
      display: flex;
      flex-direction: column;
      flex: 1;
      min-width: 0;
  }

  .thread-message-sender {
      font-size: 15px;
      font-weight: 500;
      color: var(--text-primary);
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
  }

  .thread-message-address {
      font-size: 12px;
      color: var(--text-secondary);
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
  }

  .thread-message-date {
      font-size: 12px;
      color: var(--text-secondary);
      flex-shrink: 0;
  }

  .thread-message-body {
      padding: 18px;
      font-size: 14px;
      line-height: 1.55;
      color: var(--text-primary);
      overflow-x: auto;
  }

  .thread-message-body pre {
      white-space: pre-wrap;
      font-family: inherit;
      margin: 0;
  }

  .thread-message-body p {
      margin: 0 0 10px 0;
  }

  .thread-message-body p:last-child {
      margin-bottom: 0;
  }

  .thread-message-body a {
      color: var(--accent-blue);
      text-decoration: underline;
  }

  .thread-message-attachments {
      display: flex;
      flex-direction: column;
      gap: 8px;
      padding: 0 18px 18px 18px;
      border-top: var(--divider-thickness) solid var(--divider-color);
      padding-top: 12px;
  }

  .thread-message-attachment {
      display: flex;
      align-items: center;
      gap: 10px;
      padding: 8px 12px;
      background: var(--bg-color);
      border-radius: 6px;
      font-size: 13px;
      color: var(--text-primary);
  }

  .thread-message-attachment-icon {
      font-size: 18px;
      color: var(--text-secondary);
  }

  .thread-message-attachment-name {
      flex: 1;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
  }

  .thread-message-attachment-size {
      font-size: 11px;
      color: var(--text-secondary);
      flex-shrink: 0;
  }

  .thread-detail-empty {
      display: flex;
      flex: 1;
      align-items: center;
      justify-content: center;
      color: var(--text-secondary);
      font-size: 14px;
  }
  ```

- [ ] **Step 5.2: Build and verify CSS validity**

  Run: `cargo build -p inboxly 2>&1 | tail -3`
  Expected: clean (CSS is loaded as an asset; build doesn't validate it but should still succeed).

- [ ] **Step 5.3: Commit**

  ```bash
  git add inboxly-ui/assets/main.css
  git commit -m "feat(ui): add CSS foundation for thread detail view (M34 phase 5)"
  ```

### Phase 6: `ThreadMessage` component (renders one message)

- [ ] **Step 6.1: Create `inboxly-ui/src/components/thread_message.rs`**

  ```rust
  //! Renders a single message inside the thread detail view.
  //!
  //! Avatar tile + sender + date in the header, sanitised HTML or
  //! plain-text body, and an optional attachment list at the bottom.

  use dioxus::prelude::*;

  use crate::loaded_thread::LoadedMessage;
  use crate::sanitize::sanitize_html;
  use crate::theme::avatar_colors;

  #[component]
  pub fn ThreadMessage(message: LoadedMessage) -> Element {
      let avatar_letter = message
          .from_name
          .chars()
          .next()
          .unwrap_or('?')
          .to_ascii_uppercase();
      let avatar_color = avatar_colors::for_letter(avatar_letter).to_css();
      let date_display = message.date.format("%b %-d, %Y at %-I:%M %p").to_string();

      // Choose a body renderer: sanitised HTML if available, else plain text in a <pre>.
      let body = match (&message.body_html, &message.body_text) {
          (Some(html), _) => {
              let sanitised = sanitize_html(html);
              rsx! { div { class: "thread-message-body", dangerous_inner_html: "{sanitised}" } }
          }
          (None, Some(text)) => {
              let owned = text.clone();
              rsx! { div { class: "thread-message-body", pre { "{owned}" } } }
          }
          (None, None) => {
              rsx! { div { class: "thread-message-body", "(no content)" } }
          }
      };

      rsx! {
          div {
              class: "thread-message",
              div {
                  class: "thread-message-header",
                  div {
                      class: "avatar",
                      style: "background: {avatar_color};",
                      "{avatar_letter}"
                  }
                  div {
                      class: "thread-message-from",
                      span { class: "thread-message-sender", "{message.from_name}" }
                      span { class: "thread-message-address", "{message.from_address}" }
                  }
                  span { class: "thread-message-date", "{date_display}" }
              }
              {body}
              if !message.attachments.is_empty() {
                  div {
                      class: "thread-message-attachments",
                      for att in message.attachments.iter() {
                          div {
                              class: "thread-message-attachment",
                              span { class: "thread-message-attachment-icon", "\u{1F4CE}" }
                              span { class: "thread-message-attachment-name", "{att.filename}" }
                              span { class: "thread-message-attachment-size", "{att.size_bytes} bytes" }
                          }
                      }
                  }
              }
          }
      }
  }
  ```

- [ ] **Step 6.2: Register the module**

  In `inboxly-ui/src/components/mod.rs`, add (alphabetically — between `speed_dial_fab` and `toolbar`):
  ```rust
  pub mod thread_message;
  ```

- [ ] **Step 6.3: Build and confirm clean**

  Run: `cargo build -p inboxly-ui 2>&1 | tail -5`
  Expected: clean.

- [ ] **Step 6.4: Commit**

  ```bash
  git add inboxly-ui/src/components/thread_message.rs inboxly-ui/src/components/mod.rs
  git commit -m "feat(ui): add ThreadMessage component (M34 phase 6)"
  ```

### Phase 7: `ThreadDetailView` component (layout shell)

- [ ] **Step 7.1: Create `inboxly-ui/src/components/thread_detail_view.rs`**

  ```rust
  //! Thread detail view — header bar + scrollable list of messages.
  //!
  //! Reads `Inboxly::open_thread`. If `None`, returns empty rsx (caller
  //! should not render this component when no thread is open). Each
  //! message is rendered via `ThreadMessage`. The header has a back
  //! button that dispatches `Message::CloseThread`.

  use dioxus::prelude::*;

  use crate::app::{Inboxly, Message};
  use crate::components::thread_message::ThreadMessage;

  #[component]
  pub fn ThreadDetailView() -> Element {
      let mut app_state = use_context::<Signal<Inboxly>>();
      let state = app_state.read();
      let Some(thread) = state.open_thread.clone() else {
          return rsx! {};
      };
      drop(state);

      rsx! {
          div {
              class: "thread-detail-view",
              div {
                  class: "thread-detail-header",
                  button {
                      class: "thread-detail-back",
                      aria_label: "Back to inbox",
                      onclick: move |evt: Event<MouseData>| {
                          evt.stop_propagation();
                          app_state.write().update(Message::CloseThread);
                      },
                      "\u{2190}"  // ← left arrow
                  }
                  span { class: "thread-detail-subject", "{thread.subject}" }
              }
              if thread.messages.is_empty() {
                  div { class: "thread-detail-empty", "No messages in this thread." }
              } else {
                  for message in thread.messages.iter() {
                      ThreadMessage { message: message.clone() }
                  }
              }
          }
      }
  }
  ```

- [ ] **Step 7.2: Register the module**

  In `inboxly-ui/src/components/mod.rs`, add (alphabetically — after `thread_message`):
  ```rust
  pub mod thread_detail_view;
  ```

- [ ] **Step 7.3: Wire ThreadDetailView into ContentArea**

  In `inboxly-ui/src/components/content_area.rs`, modify the Inbox arm of the `match view` block. Currently:
  ```rust
  ActiveView::Inbox => {
      if inbox_empty {
          rsx! { InboxZero {} }
      } else {
          rsx! { InboxFeed {} }
      }
  }
  ```

  Replace with:
  ```rust
  ActiveView::Inbox => {
      if thread_open {
          rsx! { ThreadDetailView {} }
      } else if inbox_empty {
          rsx! { InboxZero {} }
      } else {
          rsx! { InboxFeed {} }
      }
  }
  ```

  Add to the prologue (alongside `inbox_empty`):
  ```rust
  let thread_open = state.open_thread.is_some();
  ```

  Add the import at the top:
  ```rust
  use crate::components::thread_detail_view::ThreadDetailView;
  ```

- [ ] **Step 7.4: Build**

  Run: `cargo build -p inboxly 2>&1 | tail -3`
  Expected: clean.

- [ ] **Step 7.5: Commit**

  ```bash
  git add inboxly-ui/src/components/thread_detail_view.rs inboxly-ui/src/components/mod.rs inboxly-ui/src/components/content_area.rs
  git commit -m "feat(ui): add ThreadDetailView component and wire into ContentArea (M34 phase 7)"
  ```

### Phase 8: Wire EmailRow row-click → OpenThread

- [ ] **Step 8.1: Add row-level onclick to `email_row.rs`**

  In `inboxly-ui/src/components/email_row.rs`, locate the outer `.email-row` div. It currently has `oncontextmenu` but no `onclick`. Add an `onclick` handler that:
  1. Calls `evt.stop_propagation()` (to avoid bubbling into the content area's account-switcher dismiss handler)
  2. Dispatches `Message::OpenThread(thread_id)` using a fresh `Arc::clone` of the thread_id

  Add a new prologue clone alongside the existing `tid_*` bindings:
  ```rust
  let tid_open = Arc::clone(&thread_id);
  ```

  And add the handler to the div:
  ```rust
  onclick: move |evt: Event<MouseData>| {
      evt.stop_propagation();
      app_state.write().update(Message::OpenThread(tid_open.to_string()));
  },
  ```

  **Important:** the `oncontextmenu` handler must remain, and the existing `tid_*` clones must NOT be touched. This is purely an additive change.

- [ ] **Step 8.2: Add a state-machine test for the open/close cycle**

  In `inboxly-ui/src/app.rs::tests`, add:
  ```rust
  #[test]
  fn open_thread_sets_open_thread_field() {
      let mut app = Inboxly::default();
      assert!(app.open_thread.is_none());
      let _ = app.update(Message::OpenThread("t1".into()));
      let opened = app.open_thread.as_ref().expect("thread should be open");
      assert_eq!(opened.thread_id, "t1");
      assert_eq!(opened.messages.len(), 2); // demo_thread has two messages
  }

  #[test]
  fn close_thread_clears_open_thread_field() {
      let mut app = Inboxly::default();
      let _ = app.update(Message::OpenThread("t1".into()));
      let _ = app.update(Message::CloseThread);
      assert!(app.open_thread.is_none());
  }

  #[test]
  fn open_thread_dismisses_open_menus() {
      let mut app = Inboxly::default();
      let _ = app.update(Message::OpenContextMenu {
          thread_id: "t1".into(),
          sender_address: "a@b.com".into(),
          position: Point::ORIGIN,
      });
      assert_eq!(app.context_menu_thread, Some("t1".into()));
      let _ = app.update(Message::OpenThread("t1".into()));
      assert!(app.context_menu_thread.is_none());
      assert!(app.menu_thread_sender.is_none());
  }
  ```

- [ ] **Step 8.3: Run tests**

  Run: `cargo test -p inboxly-ui 2>&1 | grep "test result" | head -2`
  Expected: 245 passing (242 prior + 3 new) or similar.

- [ ] **Step 8.4: Commit**

  ```bash
  git add inboxly-ui/src/components/email_row.rs inboxly-ui/src/app.rs
  git commit -m "feat(ui): wire EmailRow click to OpenThread + state tests (M34 phase 8)"
  ```

### Phase 9: Escape key dismisses the open thread

- [ ] **Step 9.1: Inspect `keyboard.rs` for the existing shortcut handler**

  Run: `grep -n "Escape\|fn handle_key\|fn key_pressed" inboxly-ui/src/keyboard.rs | head -20`

  Find where keyboard events are routed to message dispatches. Currently the file mostly defines the `ShortcutMap` and `ShortcutAction` enum.

- [ ] **Step 9.2: Add a `Close` action**

  Check whether `ShortcutAction` has an existing `Close` or `Escape` variant. If yes, find the message it dispatches and ensure it covers the open-thread case. If no, the simplest path for M34 is to handle Escape in the App component's keyboard event handler directly (without going through `ShortcutMap`).

  **Actual implementation:** since `keyboard.rs` is shortcut-map config and doesn't dispatch messages directly, the cleanest route is to add an `onkeydown` handler on the `.app-shell` div in `components/app.rs` that intercepts Escape and dispatches `Message::CloseThread` when `open_thread.is_some()`.

  In `inboxly-ui/src/components/app.rs`, add to the `.app-shell` div:
  ```rust
  onkeydown: move |evt: Event<KeyboardData>| {
      if evt.key() == Key::Escape {
          let state = app_state.read();
          if state.open_thread.is_some() {
              drop(state);
              app_state.write().update(Message::CloseThread);
          }
      }
  },
  ```

  Add the `Key` import at the top of the file:
  ```rust
  use dioxus::prelude::Key;
  ```

  **Note:** the div must have `tabindex: 0` so it can receive keyboard focus. Add that attribute too:
  ```rust
  tabindex: 0,
  ```

- [ ] **Step 9.3: Build**

  Run: `cargo build -p inboxly 2>&1 | tail -3`
  Expected: clean.

  If `Key` is not in `dioxus::prelude` for 0.7.4, look up the correct path with `grep -n "pub use.*Key" /home/alan/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-html-0.7.4/src/lib.rs` and adapt. The type may be at `dioxus::prelude::keyboard::Key` or `dioxus_html::input_data::keyboard_types::Key`.

- [ ] **Step 9.4: Commit**

  ```bash
  git add inboxly-ui/src/components/app.rs
  git commit -m "feat(ui): Escape key closes open thread (M34 phase 9)"
  ```

### Phase 10: Verification + final polish

- [ ] **Step 10.1: Run the full UI test suite**

  Run: `cargo test -p inboxly-ui 2>&1 | grep "test result" | head -3`
  Expected: 248+ passing (242 prior + 3 sanitize + 3 demo_thread + 3 state-machine = 251), 0 failures.

- [ ] **Step 10.2: Run workspace clippy**

  Run: `cargo clippy --workspace -- -D warnings 2>&1 | tail -5`
  Expected: clean.

- [ ] **Step 10.3: Run workspace tests**

  Run: `cargo test --workspace 2>&1 | grep "test result" | head -10`
  Expected: all crates pass.

- [ ] **Step 10.4: Manual visual check (no automation possible)**

  This is a CHECKLIST item, not an automated step. Run `cargo run -p inboxly` in a terminal yourself, then:
  - Click any "demo" email row → ThreadDetailView opens with two demo messages
  - First message has HTML body (sanitised)
  - Second message has plain-text body in a `<pre>`
  - First message shows the demo PDF attachment in the attachment list
  - Click the back arrow → returns to inbox
  - Open another thread → press Escape → returns to inbox
  - Verify dark mode applies to all the new classes

  **Note:** since the inbox is empty without sync, you won't have email rows to click. To test the path end-to-end, temporarily wire a "demo" feed in `Inboxly::default()` OR use the developer console to dispatch `OpenThread("test")` directly. This is a manual verification helper, not part of the merged commit.

- [ ] **Step 10.5: Commit any final fixes from the manual check**

  If the visual check surfaces issues (e.g., a CSS class with the wrong name, an icon that doesn't render, an aria-label missing), fix them and commit:
  ```bash
  git add <files>
  git commit -m "fix(ui): M34 manual-check cleanups"
  ```

  If the visual check passes cleanly, no commit needed.

---

## Critical Files

- `inboxly-ui/src/app.rs` — `Inboxly` struct + `Message` enum + `update()` handlers
- `inboxly-ui/src/loaded_thread.rs` — new module: `LoadedThread`, `LoadedMessage`, `load_thread()`, `demo_thread()`
- `inboxly-ui/src/sanitize.rs` — new module: `sanitize_html()` wrapper
- `inboxly-ui/src/components/thread_detail_view.rs` — new component: layout shell
- `inboxly-ui/src/components/thread_message.rs` — new component: single message
- `inboxly-ui/src/components/email_row.rs` — add row-level `onclick` dispatching `OpenThread`
- `inboxly-ui/src/components/content_area.rs` — branch on `open_thread.is_some()` inside Inbox arm
- `inboxly-ui/src/components/app.rs` — Escape key handler on `.app-shell`
- `inboxly-ui/src/components/mod.rs` — register two new component modules
- `inboxly-ui/assets/main.css` — add ~150 lines of thread-detail CSS
- `Cargo.toml` (workspace) and `inboxly-ui/Cargo.toml` — add `ammonia` dependency

## Reusable Existing Code

- `Store::get_emails_by_thread(thread_id) -> Result<Vec<EmailRow>>` (`inboxly-store/src/emails.rs:99`) — already returns the per-thread row list
- `MaildirStore::read_email_content(&Path) -> Result<EmailContent>` (`inboxly-store/src/maildir_store.rs:578`) — already parses .eml into body/html/headers/attachments
- `EmailContent { body_text, body_html, headers, attachments }` (`inboxly-core/src/email.rs:77`) — the canonical "loaded message" type, just needs UI mapping
- `AttachmentMeta { filename, mime_type, size_bytes }` (`inboxly-core/src/attachment.rs:5`) — for the attachment list rendering
- `theme::avatar_colors::for_letter(letter).to_css()` — already used by `EmailRow`, reused for the per-message avatar tile
- `Inboxly::close_menus()` helper from M33 Phase 7A — call from `OpenThread` handler so opening a thread also dismisses any open menu
- The `Arc<str>` clone pattern from M33 — apply in `EmailRow`'s new `onclick` handler

---

## Verification

1. `cargo test --workspace` — all crates pass, ~251+ inboxly-ui tests
2. `cargo clippy --workspace -- -D warnings` — clean
3. `cargo build -p inboxly` — clean
4. **Manual visual check** (see Phase 10.4): demo thread renders, back button works, Escape works, dark mode applies, no XSS in sanitised HTML

## Out of Scope (deferred to future milestones)

- **Real attachment download** — clicking an attachment is a no-op in M34. Download is M37 (was v2 §M35).
- **Inline reply UI** — M36 (was v2 §M34).
- **Quoted-content collapse** ("show trimmed content") — defer to a polish milestone.
- **Link-click interception** (open in system browser via `open::that()`) — DEFER. Risk: clicking a link inside an email body navigates the WebKitGTK webview to the URL, replacing the app contents. Mitigation: ammonia strips `javascript:` URLs by default. Proper fix is a Dioxus webview navigation policy hook, deferred to a polish milestone.
- **Async loading via `use_resource` + `spawn_blocking`** — synchronous loader is fine at M33 data scale; async is future work.
- **Real IMAP sync wiring** — M34 ships with the demo loader as the only path that actually produces data. A future milestone (likely M35 or a dedicated sync-wiring milestone) connects `Inboxly::store` and `Inboxly::maildir` to a running sync.

## Eng Review Decisions Captured Up-Front

- **Single thread at a time.** No tabs / split views — opening a new thread replaces the current one.
- **Synchronous loader.** Async deferred until performance becomes a problem.
- **Demo loader is part of the shipped code**, not a test fixture, because it's the only way to render the view in M34's no-sync environment. Tagged with `pub fn demo_thread` so it's discoverable. When real sync lands, the demo loader can be moved behind `#[cfg(test)]` or a debug feature flag.
- **HTML sanitisation via `ammonia` defaults.** Custom whitelist deferred until we have a real email that breaks the default rules.
- **`dangerous_inner_html` is acceptable here** because the input has already been sanitised by `ammonia`. The "dangerous" name is enforced by Dioxus's API to make this decision visible at every call site.
- **`open_thread` lives on `Inboxly`** rather than in component-local state, so the back button + Escape both go through the message bus and the state machine remains the single source of truth.
- **Tests are state-machine + pure-function.** No Dioxus SSR rendering tests (Dioxus 0.7 has no usable SSR test harness for this case).
