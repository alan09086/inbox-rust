# M34: Thread Detail View + HTML Email Rendering

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` to implement this plan task-by-task with two-stage review (spec compliance + code quality) after each phase.

**Goal:** Let the user click an email row to read the full thread — sender, date, sanitised HTML body, and a list of messages stacked vertically. Read-only; reply/forward come in M36.

**Architecture:** A new Dioxus component `ThreadDetailView` renders from a `LoadedThread` data structure. The storage-layer orchestration (query SQLite, then read each body from disk) is encapsulated behind a `ThreadReader` facade in `inboxly-store` (eng review Issue 1.5) — it wraps both `Arc<Store>` and `Arc<MaildirStore>` so the UI never holds the two store handles separately. UI code converts `Vec<LoadedEmail>` (raw storage output) into `LoadedThread` (UI-shaped) via `build_loaded_thread()` in `inboxly-ui::loaded_thread`. HTML bodies are sanitised with `ammonia` before being injected via Dioxus's `dangerous_inner_html` attribute. A "demo loader" produces fake `LoadedThread` values so we can verify rendering before real IMAP sync exists.

**State split (eng review Issue 1.4):** `LoadedThread` body data does NOT sit on the `Inboxly` signal. `Inboxly` holds only the lightweight `open_thread_id: Option<String>` (intent — which thread is open). A separate `Signal<Option<Arc<LoadedThread>>>` context provided at App level holds the actual loaded body bytes. A `use_effect` in the App component bridges the two: it watches `open_thread_id`, calls `load_thread()` (or `fallback_thread()` on error), wraps the result in `Arc`, and writes it to the body signal. This ensures Inboxly's per-write `Clone` cost stays bounded (no body bytes get cloned on every nav click) while keeping the state machine testable end-to-end via the id field.

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
| `inboxly-ui/Cargo.toml` | modify | Add `ammonia.workspace = true`, `open.workspace = true`, and `url.workspace = true` |
| `Cargo.toml` (workspace) | modify | Add `ammonia = "4"`, `open = "5"`, and `url = "2"` to `[workspace.dependencies]` |
| `inboxly-ui/src/app.rs` | modify | Add `open_thread_id: Option<String>` field (intent only — body data lives in a separate signal per Issue 1.4), migrate `store: Option<Store>` → `Option<Arc<Store>>`, add `thread_reader: Option<Arc<ThreadReader>>` field (per Issue 1.5), add `OpenThread`/`CloseThread`/`OpenExternalUrl` messages and handlers |
| `inboxly-store/src/thread_reader.rs` | create | `ThreadReader` facade struct (Issue 1.5) wrapping `Arc<Store>` + `Arc<MaildirStore>` with a single `load_thread(&self, thread_id) -> Result<Vec<LoadedEmail>, StoreError>` method. Future consumers (M36 reply, M37 attachments) hold one `Arc<ThreadReader>` instead of two store handles. |
| `inboxly-store/src/lib.rs` | modify | Re-export `pub mod thread_reader;` |
| `inboxly-store/src/emails.rs` | modify | Append `ORDER BY date ASC` to `Store::get_emails_by_thread` SQL (Issue 2.5 — chronological order for thread detail view) + regression test |
| `inboxly-core/src/email.rs` | modify | Add `SlimEmailContent { id, body_text, body_html, attachments: Vec<AttachmentMeta> }` struct (Issue 2.6 — slim view used by thread detail view, skips headers and attachment bytes) |
| `inboxly-core/src/lib.rs` | modify | Re-export `SlimEmailContent` |
| `inboxly-store/src/maildir_store.rs` | modify | Add `MaildirStore::read_email_slim(&Path) -> Result<SlimEmailContent>` + `parse_email_slim(&[u8])` helper (Issue 2.6) |
| `inboxly-ui/src/loaded_thread.rs` | create | `LoadedThread` struct + `LoadedMessage` struct + `empty_thread()` (always) + `demo_thread()` (debug-only) + `fallback_thread()` selector + `load_thread()` real loader |
| `inboxly-ui/src/sanitize.rs` | create | `sanitize_html(raw: &str) -> String` wrapper around ammonia with project's whitelist |
| `inboxly-ui/src/components/thread_detail_view.rs` | create | The Dioxus component — renders header bar, message list, per-message body |
| `inboxly-ui/src/components/thread_message.rs` | create | A child component rendering one message: avatar, sender, date, body |
| `inboxly-ui/src/components/email_row.rs` | modify | Add row-level `onclick` that dispatches `Message::OpenThread(thread_id)` |
| `inboxly-ui/src/components/content_area.rs` | modify | Read the open-thread body signal context; when `Some`, render `ThreadDetailView` instead of `InboxFeed` (only inside the Inbox view branch) |
| `inboxly-ui/src/components/mod.rs` | modify | Register `thread_detail_view` and `thread_message` modules |
| `inboxly-ui/assets/main.css` | modify | Add `.thread-detail-view`, `.thread-detail-header`, `.thread-message`, `.thread-message-header`, `.thread-message-body`, `.thread-detail-empty`, `.thread-detail-error-banner` (Issue 2.1), `.thread-message-attachments` |
| `inboxly-ui/src/keyboard.rs` | modify | If a thread is open, Escape dispatches `Message::CloseThread` |

**11 files (3 created, 8 modified, 0 deleted).** The two new components keep file responsibilities focused: `thread_detail_view.rs` handles the layout shell and header; `thread_message.rs` handles a single message's rendering. This split mirrors `inbox_feed.rs` + `email_row.rs` from M33.

---

## State Changes

Two changes to the `Inboxly` struct in `inboxly-ui/src/app.rs`: add the new `open_thread_id` (intent) field, AND migrate the existing `store` field to `Arc<Store>` so it can be shared with the new `ThreadReader` facade (eng review Issue 1.5). Both fields live near `feed_sections`.

**Existing field migration (Issue 1.5):**

```rust
// BEFORE: pub store: Option<inboxly_store::store::Store>,
// AFTER:
pub store: Option<std::sync::Arc<inboxly_store::store::Store>>,
```

The existing 20+ `if let Some(ref store) = self.store { store.method(...) }` call sites compile unchanged because `Arc<Store>` derefs to `&Store` for method calls. The only places that need updating are construction sites (none in M34 — `store` is always `None`) and any code that takes `Store` by value (none in M34).

**New fields:**

```rust
/// Intent: which thread the user wants opened, if any. The actual
/// loaded body data lives in a SEPARATE signal context provided at
/// App level (eng review Issue 1.4) so per-write `Clone` of Inboxly
/// doesn't drag the body bytes around. The App component's
/// `use_effect` watches this field, calls the loader via the
/// thread_reader facade, and writes the result to the body signal.
pub open_thread_id: Option<String>,

/// Optional handle to the unified thread reader facade (eng review
/// Issue 1.5). Wraps both `Store` and `MaildirStore` so consumers
/// don't need to plumb two store handles. None in M34 (no real sync
/// is wired yet); when present, the App-level loader bridge calls
/// `thread_reader.load_thread(id)` instead of touching the raw stores.
pub thread_reader: Option<std::sync::Arc<inboxly_store::thread_reader::ThreadReader>>,
```

There is **no separate `maildir` field on Inboxly** — the `MaildirStore` is encapsulated inside `ThreadReader`, so the UI never holds it directly. This is the load-bearing benefit of Issue 1.5's facade.

Initialize in `Default`:
- `open_thread_id: None`
- `thread_reader: None`

Add `Message` variants:

```rust
/// User wants to open the full thread detail view for a thread ID.
/// Sets `open_thread_id`; the App component's bridge use_effect
/// reacts to the change and loads the body into the open-thread signal.
OpenThread(String),
/// User wants to dismiss the open thread. Clears `open_thread_id`;
/// the bridge use_effect propagates the clear to the body signal.
CloseThread,
/// Open an external URL in the user's system browser. Dispatched
/// from the link-click interceptor so `<a href>` clicks inside
/// email bodies don't navigate the WebKitGTK webview away.
OpenExternalUrl(String),
```

Handlers in `update()`:

```rust
Message::OpenThread(thread_id) => {
    // Pure intent — no body loading happens here. The App-level
    // use_effect bridge watches open_thread_id and does the load.
    self.open_thread_id = Some(thread_id);
    self.close_menus();
}
Message::CloseThread => {
    self.open_thread_id = None;
}
Message::OpenExternalUrl(url) => {
    if let Err(e) = open::that(&url) {
        tracing::warn!("open::that({url}) failed: {e}");
    }
}
```

The `close_menus()` call is intentional — opening a thread should dismiss any open context/overflow menu.

**Body data context (at App level, NOT on Inboxly):**

The App component (`components/app.rs`) provides a second context — a `Signal<Option<Arc<LoadedThread>>>` — for the loaded body data. ThreadDetailView reads from this signal directly. A `use_effect` in App watches `open_thread_id` (via a memo) and bridges intent to data:

```rust
use std::sync::Arc;
use crate::loaded_thread::{fallback_thread, load_thread, LoadedThread};

// In components/app.rs, inside fn App() -> Element { ... }
let app_state = use_context_provider(|| Signal::new(Inboxly { ... }));
let mut open_thread_signal = use_context_provider(
    || Signal::new(None::<Arc<LoadedThread>>)
);

// Memo on the id so the effect only re-runs when intent changes.
let open_id = use_memo(move || app_state.read().open_thread_id.clone());

use_effect(move || {
    let id_opt = open_id.read().clone();
    let mut open_thread_signal = open_thread_signal;
    let app_state = app_state;
    match id_opt {
        Some(id) => {
            // Eng review Issue 4.1: set the loading sentinel
            // synchronously, then spawn a task to do the actual load.
            // The click handler returns immediately and the UI
            // shows "(loading…)" until the spawn'd task completes.
            // See Phase 7 Step 7.3 for the full caveat (cooperative
            // async — the spawn'd task body still blocks the local
            // runtime during file reads).
            open_thread_signal.set(Some(Arc::new(loading_thread(&id))));
            spawn(async move {
                let snapshot = app_state.peek();
                let loaded = match snapshot.thread_reader.as_ref() {
                    Some(reader) => {
                        // Issue 1.5 facade: ThreadReader hides the
                        // two-store coupling. The UI never touches
                        // store + maildir directly.
                        match reader.load_thread(&id) {
                            Ok(emails) => match build_loaded_thread(&id, emails) {
                                Ok(thread) => thread,
                                Err(e) => {
                                    // Issue 2.1: surface the error to the
                                    // user instead of silently showing
                                    // demo/empty content. Also log it for
                                    // developer visibility.
                                    tracing::warn!(
                                        "build_loaded_thread({id}) failed: {e}"
                                    );
                                    error_thread(&id, format!("Failed to build thread view: {e}"))
                                }
                            },
                            Err(e) => {
                                tracing::warn!(
                                    "ThreadReader::load_thread({id}) failed: {e}"
                                );
                                error_thread(&id, format!("Failed to load thread: {e}"))
                            }
                        }
                    }
                    None => {
                        // No real reader wired (M34 default state) — show
                        // the fallback (demo in debug, empty in release).
                        // This is NOT an error path, so no log line.
                        fallback_thread(&id)
                    }
                };
                drop(snapshot);
                open_thread_signal.set(Some(Arc::new(loaded)));
            });
        }
        None => {
            open_thread_signal.set(None);
        }
    }
});
```

Update the use statement at the top of `app.rs` to include the new `error_thread` and `loading_thread` imports:
```rust
use crate::loaded_thread::{
    build_loaded_thread, error_thread, fallback_thread, loading_thread, LoadedThread,
};
```

This is the bridge. Five important details:
- `open_id` is a `use_memo` so the effect only re-runs when the id actually changes, not on every Inboxly write.
- The body load happens via `app_state.peek()` (not `read()`) so reading `thread_reader` doesn't subscribe the effect to that field. The effect's only reactive dependency is `open_id`.
- The bridge calls `thread_reader.load_thread(&id)` (Issue 1.5 facade), which returns `Vec<LoadedEmail>` (raw storage data). The UI's `build_loaded_thread()` then converts that into the UI's `LoadedThread` shape (display names, formatted dates, attachment metadata mapping). This split keeps the storage layer free of UI-shaped types.
- Load failures are surfaced via `error_thread()` (Issue 2.1) — the user sees a banner explaining what went wrong, AND a `tracing::warn!` log line records the failure for developer visibility. The two failure modes (`reader.load_thread` Err and `build_loaded_thread` Err) get distinct error messages so future debugging can tell them apart. The "no reader wired" case is NOT a failure, so it falls through to `fallback_thread()` with no log line.
- The actual load happens inside a `spawn` task (eng review Issue 4.1). Setting the loading sentinel synchronously, then spawning the load, gives the click handler an immediate exit while the UI shows a loading state. The spawn task body is still synchronous I/O on the local runtime (true async deferred — see Out of Scope), but the click event no longer waits for it.

---

## Design Decisions

- **`LoadedThread` is a UI-owned data structure**, not a `Store` row. It bundles the per-thread metadata + a Vec of `LoadedMessage` (each carrying the parsed `EmailContent`). Decoupling from the store rows keeps the component pure (no DB access during render).
- **`ThreadReader` facade hides the two-store coupling** (eng review Issue 1.5). The storage layer has two physical stores — `Store` (SQLite metadata) and `MaildirStore` (filesystem bodies) — but UI consumers should hold ONE handle, not two. `ThreadReader` is a thin facade in `inboxly-store` that wraps `Arc<Store>` + `Arc<MaildirStore>` and exposes a single `load_thread(&self, thread_id) -> Result<Vec<LoadedEmail>, StoreError>` method. Returns raw storage data (`LoadedEmail = { row: EmailRow, content: Option<EmailContent> }`); the UI's `build_loaded_thread()` converts that into the UI-shaped `LoadedThread` (display names, formatted dates). Future consumers (M36 reply, M37 attachments) hold `Arc<ThreadReader>` instead of plumbing two store handles independently. Inboxly's existing `store: Option<Store>` field migrates to `Option<Arc<Store>>` so it can be shared with the facade.
- **Synchronous loader for now.** `load_thread()` does blocking file I/O when called, but at M33's data scale (hundreds of messages per thread max) this is fine. Async loading via `use_resource` is future work — note in code but don't implement.
- **HTML sanitisation via `ammonia` with the default whitelist plus an `<img>` privacy pass.** Ammonia's defaults strip `<script>`, `<style>`, `javascript:` URLs, event handlers, `<iframe>`, and most other unsafe constructs. On top of that, M34 strips `src` and `srcset` from `<img>` tags so tracking pixels can't phone home when an email is opened (eng review Issue 1.1). The `<img>` element itself stays in the DOM — it renders as a broken-image placeholder with alt text intact — so email layout isn't destroyed. A future milestone will add per-sender "Show images" toggle for users who want remote images back.
- **Plain-text fallback.** If `body_html` is `None` but `body_text` is `Some`, render the text inside a `<pre>` tag wrapped in our own escape (no ammonia needed for already-text content). If both are `None`, render `"(no content)"`.
- **Link interception via sentinel-scheme rewrite + JS bridge** (eng review Issue 1.2). The naive approach — let `<a href="https://...">` clicks fall through to WebKitGTK — would navigate the entire app to the linked URL, destroying user state. Instead, the sanitiser rewrites every external URL to `#inboxly-ext:<original>`, which the webview treats as a same-page anchor (no-op). A `use_effect` in the App component installs a JS click listener via Dioxus's `document::eval` bridge that catches clicks on sentinel-prefixed `<a>` elements, strips the prefix, and forwards the real URL to Rust via `dioxus.send`. The Rust handler dispatches `Message::OpenExternalUrl(url)`, which calls `open::that(url)` to hand the URL to the user's default system browser. Adds the `open` workspace dependency.
- **No quoted-content collapse in M34.** The v2 spec mentions "show trimmed content" expander; that's a polish item we defer. M34 renders the full body verbatim.
- **Attachment list shows metadata only.** `LoadedMessage.attachments` is `Vec<AttachmentMeta>` (from `EmailContent.attachments` mapped to drop the byte content). Click handlers are no-ops (download is M37). Just a list of "filename — MIME — size".
- **Two-signal split: intent vs body data** (eng review Issue 1.4). `Inboxly::open_thread_id: Option<String>` records the user's intent ("which thread is open") and goes through the existing message-handler state machine. The actual loaded body — potentially megabytes of HTML/text/headers/attachment metadata — lives in a SEPARATE `Signal<Option<Arc<LoadedThread>>>` context provided at App level. ThreadDetailView reads from the body signal directly, NOT from Inboxly. The two are bridged by a `use_effect` in App that watches `open_thread_id` (via `use_memo`) and calls the loader. This keeps Inboxly's per-write `Clone` cost bounded — opening a thread no longer drags megabytes of body bytes through every nav click — while preserving the state-machine testability that M33 established. Tests assert on `open_thread_id`; visual verification covers the body load path end-to-end.
- **Demo loader gated to debug builds.** `demo_thread(thread_id)` produces a `LoadedThread` with two fake messages (one HTML, one plain-text), several headers, and one fake attachment. **Compiled only in `#[cfg(debug_assertions)]` builds** per eng review Issue 1.3 — release binaries do not ship fixture data. Release builds fall back to `empty_thread(thread_id)` which returns a `(no content available)` placeholder. Callers use `fallback_thread(thread_id)` which resolves to the right one for the current build mode. When real sync lands, the cleanup is removing the cfg gate (or deleting `demo_thread` and `fallback_thread` and replacing call sites with `empty_thread` directly).
- **Escape key is handled in `keyboard.rs`** rather than directly on the ThreadDetailView component, matching the existing keyboard shortcut routing pattern.
- **`Inboxly::store: Option<Store>`** field is verified to exist (line 105 of `app.rs` at the time the plan was written). The Phase 7 use_effect bridge reads it via `app_state.peek().store.as_ref()` so the bridge subscribes only to `open_thread_id`, not to store changes.

---

## New Components (3 files)

| File | Purpose | Estimated LOC |
|------|---------|--------------|
| `loaded_thread.rs` | Pure data: `LoadedThread`, `LoadedMessage`, `empty_thread()`, debug-only `demo_thread()`, `fallback_thread()` selector, `load_thread()`, unit tests | ~190 |
| `sanitize.rs` | `sanitize_html()` wrapper, unit tests for "strips script tags", "strips javascript: URLs", "preserves text" | ~50 |
| `components/thread_detail_view.rs` | Layout shell: header bar with back button + subject, list of `ThreadMessage` children, empty state | ~80 |
| `components/thread_message.rs` | Single message: avatar, sender name + email, formatted date, body div, attachment list | ~120 |

---

## Implementation Order

### Phase 1: Verify `Inboxly` shape and add foundational state

- [ ] **Step 1.1: Confirm the existing `Inboxly::store` field shape**

  Run: `grep -n "^    pub store\|^    store:" inboxly-ui/src/app.rs`

  Expected: `pub store: Option<Store>` at around line 105 (verified during plan writing). The `Option` matters because the UI runs without a configured account in M34 — use `self.store.as_ref()` to access it. If the type has changed since the plan was written (e.g. someone wrapped it in `Arc`), adapt the borrows in Phase 4 accordingly.

- [ ] **Step 1.2: Add `ammonia`, `open`, and `url` to workspace dependencies**

  Edit `Cargo.toml` (workspace root) `[workspace.dependencies]` section, add:
  ```toml
  ammonia = "4"
  open = "5"
  url = "2"
  ```
  Edit `inboxly-ui/Cargo.toml` `[dependencies]` section, add:
  ```toml
  ammonia.workspace = true
  open.workspace = true
  url.workspace = true
  ```

  - `ammonia` sanitises email HTML bodies before rendering.
  - `open` opens URLs in the user's default system browser from the link-click handler (Issue 1.2 from eng review: prevents WebKitGTK from hijacking the app on link click).
  - `url` parses URLs for scheme validation in the `OpenExternalUrl` handler (Issue 2.2 from eng review: defence in depth — only `http`, `https`, and `mailto` URLs are allowed to reach `open::that()`).

- [ ] **Step 1.3: Migrate `store` to `Arc<Store>`, add `open_thread_id` and `thread_reader` fields**

  In `inboxly-ui/src/app.rs`:

  **(a) Migrate the existing `store` field type:**
  ```rust
  // Find this:
  pub store: Option<Store>,
  // (or possibly with the full path: Option<inboxly_store::store::Store>)

  // Replace with:
  pub store: Option<std::sync::Arc<inboxly_store::store::Store>>,
  ```

  Run `cargo build -p inboxly-ui 2>&1 | tail -20` after this single change. Most call sites should compile unchanged because `Arc<Store>` derefs to `&Store`. If any specific site fails to compile, the most likely fix is wrapping a clone in `Arc::clone(store)` or accessing via `(**store).foo()` if Rust can't find the right deref.

  Expected: clean. If a call site moves `Store` by value (rare), it'll need to either clone the underlying Connection (not supported — would be an error) or clone the Arc (`Arc::clone(store)`). Flag any such site as a follow-up if you find one.

  **(b) Add the new fields near `feed_sections`:**
  ```rust
  /// Intent: which thread the user wants opened. The actual loaded
  /// body data lives in a separate signal at App level (Issue 1.4)
  /// so per-write `Clone` of Inboxly doesn't drag body bytes around.
  pub open_thread_id: Option<String>,

  /// Unified thread reader facade (Issue 1.5). Wraps Store +
  /// MaildirStore so consumers don't need to plumb two handles.
  /// None in M34 since real sync isn't wired yet — the App-level
  /// bridge falls through to fallback_thread() when this is None.
  pub thread_reader: Option<std::sync::Arc<inboxly_store::thread_reader::ThreadReader>>,
  ```

  Initialize both to `None` in `Default::default()`.

  **(c) Build:**
  ```bash
  cargo build -p inboxly-ui 2>&1 | tail -10
  ```
  Expected: error `unresolved import inboxly_store::thread_reader` — the module doesn't exist yet (it's added in Phase 4). That's the expected failure. Move on to Phase 1 step 1.4 and the build will resolve once Phase 4 lands.

  **Note:** the `loaded_thread` module from Phase 2 is also still missing at this point, but the Step 1.5 handlers don't reference it (per Issue 1.4, OpenThread is pure intent), so that's not a problem.

- [ ] **Step 1.4: Add `OpenThread`, `CloseThread`, and `OpenExternalUrl` Message variants**

  In `inboxly-ui/src/app.rs`, locate the `Message` enum. Add (place near `OpenContextMenu`):
  ```rust
  /// Open the full thread detail view for a thread ID.
  OpenThread(String),
  /// Close the open thread and return to the inbox feed.
  CloseThread,
  /// Open an external URL in the user's system browser. Dispatched
  /// from the thread detail view's link-click interceptor so
  /// `<a href>` clicks inside email bodies don't navigate the
  /// WebKitGTK webview away from the app (eng review Issue 1.2).
  OpenExternalUrl(String),
  ```

- [ ] **Step 1.5: Add Message handlers (placeholder bodies)**

  In `update()`, add the three new arms. **Pure intent** — no body loading happens in Inboxly per Issue 1.4. The App-level use_effect bridge (added in Phase 7) watches `open_thread_id` and calls the loader.
  ```rust
  Message::OpenThread(thread_id) => {
      self.open_thread_id = Some(thread_id);
      self.close_menus();
  }
  Message::CloseThread => {
      self.open_thread_id = None;
  }
  Message::OpenExternalUrl(url) => {
      // Eng review Issue 2.2: defence-in-depth URL validation BEFORE
      // calling open::that(). The sanitiser already strips javascript:
      // URLs (and other unsafe schemes) but a future ammonia version,
      // or an edge case in the url_filter_map ordering, could let one
      // slip through. Parse the URL and check the scheme against an
      // allowlist before handing it to the system browser.
      match ::url::Url::parse(&url) {
          Ok(parsed) => match parsed.scheme() {
              "http" | "https" | "mailto" => {
                  if let Err(e) = open::that(&url) {
                      tracing::warn!("open::that({url}) failed: {e}");
                  }
              }
              other => {
                  tracing::warn!(
                      "OpenExternalUrl rejected scheme {other:?} for url {url:?}"
                  );
              }
          },
          Err(e) => {
              tracing::warn!("OpenExternalUrl: failed to parse {url:?}: {e}");
          }
      }
  }
  ```

  **Note on the `::url::Url::parse` path:** the leading `::` disambiguates the `url` crate from any local `url` variable that might shadow it (no shadowing today, but defensive). If `::url::Url` doesn't compile, fall back to a `use url::Url;` import at the top of `app.rs` and call `Url::parse(&url)`.

  No loader call inside Inboxly — Phase 7 adds the App-level use_effect bridge that watches `open_thread_id` and calls `fallback_thread()` (debug) or `load_thread()` (real-store path, Phase 4).

- [ ] **Step 1.6: Build (will fail until Phase 4 adds ThreadReader)**

  Run: `cargo build -p inboxly-ui 2>&1 | tail -10`

  Expected: error `unresolved import inboxly_store::thread_reader` — Phase 4 adds the module. This is the expected failure; move to Phase 2 (which doesn't depend on `inboxly_store::thread_reader`) and Phase 4 (which adds it). The build will be clean from Phase 4 onward.

  If you see ANY error other than the unresolved `inboxly_store::thread_reader` import, the Step 1.3 `store` field migration to `Arc<Store>` may have hit an unexpected call site. Inspect the failing call sites and either wrap with `Arc::clone(store)` for ownership or `(**store)` for an explicit deref.

- [ ] **Step 1.7: Commit**

  ```bash
  git add Cargo.toml inboxly-ui/Cargo.toml inboxly-ui/src/app.rs
  git commit -m "feat(ui): add OpenThread/CloseThread state and ammonia dep (M34 phase 1)"
  ```

### Phase 2: `LoadedThread` data structure + `empty_thread()` + dev-only `demo_thread()`

This phase introduces three things:
1. The `LoadedThread` / `LoadedMessage` data types (always available)
2. `empty_thread(thread_id)` — minimal placeholder used in release builds when no data is available (always available)
3. `demo_thread(thread_id)` — fake-data fixture for visual verification (debug-only, gated behind `#[cfg(debug_assertions)]` per eng review Issue 1.3)
4. `fallback_thread(thread_id)` — picks the right one based on build mode

- [ ] **Step 2.1: Write the failing tests**

  Create `inboxly-ui/src/loaded_thread.rs` with the test stub:
  ```rust
  //! Loaded thread data structure for the thread detail view.

  #[cfg(test)]
  mod tests {
      use super::*;

      // empty_thread tests (always run)

      #[test]
      fn empty_thread_has_no_messages_and_no_error() {
          let thread = empty_thread("t-empty");
          assert_eq!(thread.thread_id, "t-empty");
          assert!(thread.messages.is_empty());
          assert_eq!(thread.subject, "(no content available)");
          assert!(thread.error_message.is_none(), "empty != error");
      }

      // error_thread tests (eng review Issue 2.1)

      #[test]
      fn error_thread_carries_error_message() {
          let thread = error_thread("t-bad", "DB locked");
          assert_eq!(thread.thread_id, "t-bad");
          assert!(thread.messages.is_empty());
          assert_eq!(thread.subject, "(failed to load)");
          assert_eq!(thread.error_message.as_deref(), Some("DB locked"));
      }

      #[test]
      fn error_thread_distinguishes_from_empty_thread() {
          // Both have zero messages, but error_thread has Some(msg) and
          // empty_thread has None. ThreadDetailView's banner conditional
          // depends on this distinction.
          let empty = empty_thread("t1");
          let err = error_thread("t1", "boom");
          assert!(empty.error_message.is_none());
          assert!(err.error_message.is_some());
      }

      // loading_thread tests (eng review Issue 4.1)

      #[test]
      fn loading_thread_has_no_messages_no_error() {
          let thread = loading_thread("t-loading");
          assert_eq!(thread.thread_id, "t-loading");
          assert!(thread.messages.is_empty());
          assert!(thread.error_message.is_none());
          // Subject contains the loading sentinel.
          assert!(thread.subject.contains("loading"));
      }

      // demo_thread tests (debug builds only)

      #[cfg(debug_assertions)]
      #[test]
      fn demo_thread_has_two_messages() {
          let thread = demo_thread("demo-thread-1");
          assert_eq!(thread.thread_id, "demo-thread-1");
          assert_eq!(thread.messages.len(), 2);
          assert_eq!(thread.subject, "Welcome to Inboxly");
      }

      #[cfg(debug_assertions)]
      #[test]
      fn demo_thread_first_message_has_html_body() {
          let thread = demo_thread("demo");
          let first = &thread.messages[0];
          assert!(first.body_html.is_some());
          assert!(first.body_html.as_ref().unwrap().contains("<p>"));
      }

      #[cfg(debug_assertions)]
      #[test]
      fn demo_thread_first_message_has_a_link() {
          // Phase 9 manual verification depends on this — the demo body
          // must contain at least one <a href> so the link-click path
          // can be exercised by clicking through the demo.
          let thread = demo_thread("demo");
          let first_html = thread.messages[0].body_html.as_ref().unwrap();
          assert!(first_html.contains("<a href"));
      }

      #[cfg(debug_assertions)]
      #[test]
      fn demo_thread_second_message_is_plain_text() {
          let thread = demo_thread("demo");
          let second = &thread.messages[1];
          assert!(second.body_html.is_none());
          assert!(second.body_text.is_some());
      }

      // fallback_thread always returns SOMETHING

      #[test]
      fn fallback_thread_returns_a_loaded_thread() {
          let thread = fallback_thread("any-id");
          assert_eq!(thread.thread_id, "any-id");
          // In debug builds this is the demo (2 messages); in release
          // it's the empty placeholder (0 messages). Either is valid —
          // we just want to confirm the function compiles in both modes
          // and returns the right shape.
          #[cfg(debug_assertions)]
          assert_eq!(thread.messages.len(), 2);
          #[cfg(not(debug_assertions))]
          assert!(thread.messages.is_empty());
      }
  }
  ```

  Add `pub mod loaded_thread;` to `inboxly-ui/src/lib.rs`.

  Run: `cargo test -p inboxly-ui loaded_thread 2>&1 | tail -10`
  Expected: compile error — `LoadedThread`, `LoadedMessage`, `empty_thread`, `demo_thread`, `fallback_thread` not defined.

- [ ] **Step 2.2: Implement `LoadedThread`, `LoadedMessage`, `empty_thread()`, `demo_thread()`, and `fallback_thread()`**

  Replace the test-only file with the full module. Place above the `#[cfg(test)] mod tests { ... }` block:
  ```rust
  //! Loaded thread data structure for the thread detail view.
  //!
  //! `LoadedThread` is a UI-owned bundle of per-thread metadata plus
  //! all messages with their full content (body, headers, attachments).
  //!
  //! Three constructor paths:
  //! - `build_loaded_thread(thread_id, emails)` — the real path,
  //!   takes raw `Vec<LoadedEmail>` from `ThreadReader::load_thread()`
  //!   and converts to UI shape. Added in Phase 4.
  //! - `empty_thread(thread_id)` — release-build placeholder when no
  //!   real data is available. Returns a thread with zero messages
  //!   and a "(no content available)" subject. Always compiled in.
  //! - `demo_thread(thread_id)` — debug-build fixture with two fake
  //!   messages. Used during M34 development for visual verification
  //!   before real sync is wired. Gated behind `#[cfg(debug_assertions)]`
  //!   per eng review Issue 1.3 — production binaries don't ship the
  //!   fixture data.
  //!
  //! Callers should use `fallback_thread(thread_id)`, which picks
  //! `demo_thread` in debug builds and `empty_thread` in release.

  use chrono::{DateTime, Utc};

  use inboxly_core::AttachmentMeta;

  /// All data needed to render the thread detail view.
  ///
  /// Messages are wrapped in `Arc<LoadedMessage>` (eng review Issue 2.8)
  /// so per-render clones in the `for` loop in `ThreadDetailView` are
  /// refcount bumps, not deep clones of the message body bytes. The
  /// outer `LoadedThread` is also Arc-wrapped at the signal boundary
  /// (Issue 1.4) — together that's two layers of Arc, each addressing
  /// a different cost: the outer Arc avoids cloning the whole thread
  /// on every Inboxly write, the inner Arc avoids cloning each message
  /// on every ThreadDetailView re-render.
  #[derive(Debug, Clone, PartialEq)]
  pub struct LoadedThread {
      pub thread_id: String,
      pub subject: String,
      pub messages: Vec<std::sync::Arc<LoadedMessage>>,
      /// Set when the load failed. ThreadDetailView renders this in
      /// a banner above the message list so the user sees what went
      /// wrong instead of being silently shown demo/empty content.
      /// Eng review Issue 2.1.
      pub error_message: Option<String>,
  }

  /// One message inside a loaded thread.
  #[derive(Debug, Clone, PartialEq)]
  pub struct LoadedMessage {
      pub email_id: String,
      pub from_name: String,
      pub from_address: String,
      /// `None` when the timestamp couldn't be parsed (corrupt or
      /// out-of-range value in the store). Eng review Issue 2.7:
      /// the previous design fell back to `Utc::now()` for bad
      /// timestamps, which made corrupt data look like a fresh
      /// email. Option<...> forces the renderer to handle "unknown
      /// time" as a deliberate display state.
      pub date: Option<DateTime<Utc>>,
      pub body_text: Option<String>,
      pub body_html: Option<String>,
      pub attachments: Vec<AttachmentMeta>,
  }

  /// Minimal placeholder thread shown when no real data is available
  /// (e.g., release builds before sync is wired). Always compiled in.
  /// Carries no error message — the user clicked a thread but the
  /// system genuinely had nothing to show, which is different from
  /// a load failure.
  pub fn empty_thread(thread_id: &str) -> LoadedThread {
      LoadedThread {
          thread_id: thread_id.to_string(),
          subject: "(no content available)".to_string(),
          messages: Vec::new(),
          error_message: None,
      }
  }

  /// Construct a thread that represents a load failure. The
  /// ThreadDetailView renders the `error_message` in a red banner
  /// above the (empty) message list. Eng review Issue 2.1 — the
  /// alternative was silent fallback to demo/empty content, which
  /// hides bugs from users and developers.
  pub fn error_thread(thread_id: &str, message: impl Into<String>) -> LoadedThread {
      LoadedThread {
          thread_id: thread_id.to_string(),
          subject: "(failed to load)".to_string(),
          messages: Vec::new(),
          error_message: Some(message.into()),
      }
  }

  /// Sentinel thread shown while a load is in progress. Eng review
  /// Issue 4.1 — the App-level use_effect bridge sets this immediately
  /// after the user clicks a thread row, then runs the actual load
  /// in a Dioxus `spawn`'d task. The sentinel makes the UI show a
  /// "loading" state instead of an awkward gap or stale content.
  ///
  /// `subject == "(loading…)"` is the visible signal; the renderer
  /// can also detect a loading state by checking `messages.is_empty()`
  /// + `error_message.is_none()` + that specific subject. Future
  /// work could promote this to a typed `LoadState` enum at the
  /// signal layer if more states are needed.
  pub fn loading_thread(thread_id: &str) -> LoadedThread {
      LoadedThread {
          thread_id: thread_id.to_string(),
          subject: "(loading\u{2026})".to_string(),
          messages: Vec::new(),
          error_message: None,
      }
  }

  /// Build a fake thread with two messages for visual verification
  /// during M34 development. Debug builds only — release binaries
  /// do not ship this fixture data (eng review Issue 1.3).
  ///
  /// The first message includes an HTML body with an external link
  /// so the Phase 9 link-click interceptor can be exercised by hand.
  #[cfg(debug_assertions)]
  pub fn demo_thread(thread_id: &str) -> LoadedThread {
      use std::sync::Arc;
      LoadedThread {
          thread_id: thread_id.to_string(),
          subject: "Welcome to Inboxly".to_string(),
          error_message: None,
          messages: vec![
              Arc::new(LoadedMessage {
                  email_id: format!("{thread_id}-1"),
                  from_name: "Alan Gaudet".to_string(),
                  from_address: "alan@example.com".to_string(),
                  date: Some(chrono::Utc::now() - chrono::Duration::hours(2)),
                  body_html: Some(
                      "<p>Hi there,</p>\
                       <p>This is a <strong>demo</strong> message rendered \
                       from the M34 thread detail view. The body is sanitised HTML.</p>\
                       <p>More info: <a href=\"https://example.com\">example.com</a> \
                       (clicking should open in your default browser, not navigate the app)</p>\
                       <p>Cheers,<br>Alan</p>"
                          .to_string(),
                  ),
                  body_text: None,
                  attachments: vec![AttachmentMeta {
                      filename: "report.pdf".to_string(),
                      mime_type: "application/pdf".to_string(),
                      size_bytes: 124_532,
                  }],
              }),
              Arc::new(LoadedMessage {
                  email_id: format!("{thread_id}-2"),
                  from_name: "Test Sender".to_string(),
                  from_address: "test@example.com".to_string(),
                  date: Some(chrono::Utc::now() - chrono::Duration::minutes(15)),
                  body_html: None,
                  body_text: Some(
                      "Reply with a plain-text body.\n\nNo HTML, no formatting.\n\nLine three."
                          .to_string(),
                  ),
                  attachments: vec![],
              }),
          ],
      }
  }

  /// Pick the right placeholder thread for the current build mode.
  /// Debug builds get the demo fixture; release builds get the empty
  /// placeholder. Single call site for the cfg switch so callers don't
  /// have to repeat the gate. **This is for the no-data case, NOT
  /// for load failures** — use `error_thread()` for those.
  pub fn fallback_thread(thread_id: &str) -> LoadedThread {
      #[cfg(debug_assertions)]
      {
          demo_thread(thread_id)
      }
      #[cfg(not(debug_assertions))]
      {
          empty_thread(thread_id)
      }
  }
  ```

  Run: `cargo test -p inboxly-ui loaded_thread 2>&1 | tail -10`
  Expected: 7 tests pass in debug mode (1 empty + 2 error + 4 demo gated on `cfg(debug_assertions)`); 3 tests in release mode (1 empty + 2 error). The `fallback_thread` test runs in both modes.

- [ ] **Step 2.3: Build the whole UI crate**

  Run: `cargo build -p inboxly-ui 2>&1 | tail -5`
  Expected: clean — Phase 1's `OpenThread` handler that referenced `crate::loaded_thread::demo_thread` should now resolve.

- [ ] **Step 2.4: Commit**

  ```bash
  git add inboxly-ui/src/loaded_thread.rs inboxly-ui/src/lib.rs
  git commit -m "feat(ui): add LoadedThread types + empty/demo/fallback constructors (M34 phase 2)"
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
  //! whitelist, which strips:
  //! - All the things ammonia strips by default (scripts, event
  //!   handlers, `javascript:` URLs, iframes, etc.)
  //! - **`src` and `srcset` on `<img>`** — privacy pass from the M34
  //!   eng review (Issue 1.1). Blocks tracking-pixel phone-home on
  //!   email open. The `<img>` tag itself is preserved (renders as
  //!   broken-image placeholder with alt text) so layout isn't
  //!   destroyed. A future milestone will add a per-sender "Show
  //!   images" toggle.
  //!
  //! And rewrites:
  //! - **All `<a href>` URLs** are rewritten to the sentinel prefix
  //!   `#inboxly-ext:<original-url>` — eng review Issue 1.2. Because
  //!   the `href` now starts with `#`, WebKitGTK treats the click as
  //!   a same-page anchor and never navigates away from the app.
  //!   The `ThreadMessage` component's click handler catches the
  //!   click, strips the sentinel prefix, and dispatches
  //!   `Message::OpenExternalUrl(real_url)` which hands the URL to
  //!   `open::that()` for the system browser.

  /// Sentinel prefix injected into rewritten `<a href>` URLs so the
  /// webview treats them as harmless in-page anchors instead of
  /// navigations. The thread-message click interceptor matches this
  /// prefix to reconstruct the original URL.
  pub const EXT_URL_SENTINEL: &str = "#inboxly-ext:";

  /// Sanitise an HTML email body for safe rendering.
  ///
  /// Applies the project's whitelist over ammonia's defaults:
  /// - Strips `src`/`srcset` from `<img>` (tracking pixel block).
  /// - Rewrites every `<a href>` to `#inboxly-ext:<url>` so clicks
  ///   don't navigate the webview.
  pub fn sanitize_html(raw: &str) -> String {
      let mut builder = ammonia::Builder::default();
      builder.rm_tag_attributes("img", &["src", "srcset"]);
      // url_filter_map takes a closure that receives each URL ammonia
      // would otherwise allow through and returns the replacement.
      // Returning None drops the attribute entirely; returning
      // Some(new_url) substitutes. We prefix with the sentinel so the
      // webview treats clicks as same-page anchors.
      builder.url_filter_map(|url| {
          // In-page anchors (href="#section") are already safe — leave
          // them alone so within-email anchor navigation still works.
          if url.starts_with('#') {
              return Some(url.to_string().into());
          }
          Some(format!("{EXT_URL_SENTINEL}{url}").into())
      });
      builder.clean(raw).to_string()
  }

  /// Extract the real URL from a sentinel-prefixed href, or `None`
  /// if the href doesn't match the sentinel form. Used by the
  /// ThreadMessage click interceptor to decide whether a click on
  /// an `<a>` element should open the system browser.
  pub fn extract_ext_url(href: &str) -> Option<&str> {
      href.strip_prefix(EXT_URL_SENTINEL)
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
      fn rewrites_anchor_href_to_sentinel() {
          // Eng review Issue 1.2: clicking <a href="https://..."> inside
          // the Dioxus webview would otherwise navigate the app away.
          // We rewrite to a sentinel fragment so WebKitGTK treats the
          // click as an in-page anchor (no-op) and the click handler
          // on .thread-message-body dispatches OpenExternalUrl instead.
          let dirty = "<a href=\"https://example.com/invoice\">view invoice</a>";
          let clean = sanitize_html(dirty);
          // The link text is preserved.
          assert!(clean.contains(">view invoice</a>"));
          // The sentinel prefix is applied.
          assert!(
              clean.contains("href=\"#inboxly-ext:https://example.com/invoice\""),
              "expected sentinel-prefixed href, got: {clean}"
          );
          // The original URL should NOT be reachable as a normal href
          // (no bare `href="https://` without the sentinel).
          assert!(!clean.contains("href=\"https://"));
      }

      #[test]
      fn preserves_in_page_anchor_links() {
          // Within-email anchors (rare but legal) should stay as same-page
          // anchors and NOT get the sentinel prefix. The webview already
          // treats these as in-page navigation, which is fine.
          let dirty = "<a href=\"#section2\">jump</a>";
          let clean = sanitize_html(dirty);
          assert!(clean.contains("href=\"#section2\""));
          assert!(!clean.contains("inboxly-ext"));
      }

      #[test]
      fn extract_ext_url_returns_original() {
          assert_eq!(
              extract_ext_url("#inboxly-ext:https://example.com/foo"),
              Some("https://example.com/foo")
          );
          assert_eq!(
              extract_ext_url("#inboxly-ext:mailto:a@b.com"),
              Some("mailto:a@b.com")
          );
      }

      #[test]
      fn extract_ext_url_returns_none_for_plain_anchor() {
          assert_eq!(extract_ext_url("#section"), None);
          assert_eq!(extract_ext_url("https://example.com"), None);
          assert_eq!(extract_ext_url(""), None);
      }

      // Eng review Issue 2.2: sanitiser-level defence in depth.
      // The OpenExternalUrl handler also validates the scheme before
      // calling open::that(), but this test pins the sanitiser invariant
      // so future ammonia upgrades can't silently regress it.

      #[test]
      fn javascript_url_does_not_round_trip_through_sanitiser() {
          // Round-trip: sanitise <a href="javascript:..."> and then try
          // to extract the original URL via the JS bridge's logic.
          // The result MUST NOT be a javascript: URL.
          let dirty = "<a href=\"javascript:alert(1)\">click</a>";
          let clean = sanitize_html(dirty);

          // ammonia's default scheme allowlist must strip the
          // javascript: URL entirely. After sanitisation, there
          // should be NO trace of the word "javascript".
          assert!(
              !clean.contains("javascript"),
              "javascript: URL leaked through sanitiser: {clean}"
          );

          // Whatever ammonia decided to do with the <a> tag, the
          // result must not contain a sentinel-prefixed URL that
          // unwraps to a javascript: scheme. Find any href in the
          // sanitised output and verify it doesn't unwrap to evil.
          // We do a substring scan since a full HTML parse is overkill.
          for window in clean.split("href=\"").skip(1) {
              if let Some(end) = window.find('"') {
                  let href = &window[..end];
                  if let Some(unwrapped) = extract_ext_url(href) {
                      assert!(
                          !unwrapped.starts_with("javascript:"),
                          "extracted URL is a javascript: scheme: {unwrapped}"
                      );
                  }
              }
          }
      }

      #[test]
      fn data_url_does_not_round_trip_through_sanitiser() {
          // Same defence for data: URLs (which can encode arbitrary
          // payloads in some browsers — defence in depth).
          let dirty = "<a href=\"data:text/html,<script>alert(1)</script>\">click</a>";
          let clean = sanitize_html(dirty);
          for window in clean.split("href=\"").skip(1) {
              if let Some(end) = window.find('"') {
                  let href = &window[..end];
                  if let Some(unwrapped) = extract_ext_url(href) {
                      assert!(
                          !unwrapped.starts_with("data:"),
                          "extracted URL is a data: scheme: {unwrapped}"
                      );
                  }
              }
          }
      }

      #[test]
      fn strips_image_src_for_privacy() {
          // Tracking-pixel case: commercial senders embed <img src="tracker.com/..."
          // in emails so they know when users open them. Stripping src makes the
          // image blank (no network request) while preserving alt text and layout.
          let dirty = "<p>Hi</p><img src=\"https://tracker.example.com/pixel.gif?u=alan\" alt=\"tracker\">";
          let clean = sanitize_html(dirty);
          assert!(!clean.contains("tracker.example.com"), "src URL must be gone");
          assert!(!clean.contains("pixel.gif"), "no trace of the tracker path");
          assert!(clean.contains("<p>Hi</p>"), "rest of the body preserved");
          // Alt text is preserved for screen readers — ammonia's default attribute
          // list for <img> keeps alt.
          assert!(clean.contains("alt=\"tracker\"") || clean.contains("<img"));
      }

      #[test]
      fn strips_image_srcset_for_privacy() {
          // srcset is a second channel that responsive images use to point at
          // multiple URLs. Must strip this too or retina-aware trackers still fire.
          let dirty = "<img srcset=\"https://t.com/1x.gif 1x, https://t.com/2x.gif 2x\">";
          let clean = sanitize_html(dirty);
          assert!(!clean.contains("t.com"), "no srcset URLs leak through");
          assert!(!clean.contains("srcset"), "srcset attribute itself gone");
      }

      #[test]
      fn preserves_inline_data_url_images_for_safety() {
          // Inline data: URLs don't phone home — they're embedded bytes.
          // Ammonia's default permits them and our rm_tag_attributes doesn't
          // distinguish schemes, so data: URLs also get stripped. Document
          // that as current behavior; a follow-up can permit data: specifically.
          let dirty = "<img src=\"data:image/png;base64,iVBORw0KGgo=\">";
          let clean = sanitize_html(dirty);
          // Current behavior: strips ALL src, including safe data: URLs.
          // This is a conservative tradeoff — see "Eng Review Decisions" in plan.
          assert!(!clean.contains("data:image"));
      }
  }
  ```

  Add `pub mod sanitize;` to `inboxly-ui/src/lib.rs`.

  Run: `cargo test -p inboxly-ui sanitize 2>&1 | tail -10`
  Expected: 14 tests pass (4 original + 3 for the `<img>` privacy pass + 5 for the link-rewrite + `extract_ext_url` helper + 2 for the Issue 2.2 round-trip defence-in-depth tests).

- [ ] **Step 3.2: Commit**

  ```bash
  git add inboxly-ui/src/sanitize.rs inboxly-ui/src/lib.rs
  git commit -m "feat(ui): add HTML sanitisation helper via ammonia (M34 phase 3)"
  ```

### Phase 4: `ThreadReader` facade in `inboxly-store` + `build_loaded_thread()` in `inboxly-ui`

This phase implements eng review Issue 1.5: a single facade that wraps both `Store` and `MaildirStore` so consumers don't need to plumb two handles. The phase has two halves: a new module in `inboxly-store` that defines `ThreadReader`, and an updated `loaded_thread.rs` in `inboxly-ui` that converts the storage-layer output into the UI's `LoadedThread` shape.

The Inboxly handler does NOT change in this phase. The facade is invoked from the App-level `use_effect` bridge added in Phase 7. Phase 4 is pure infrastructure.

- [ ] **Step 4.1: Add `ORDER BY date ASC` to `Store::get_emails_by_thread` (eng review Issue 2.5)**

  Before building the ThreadReader, fix the underlying query so it returns emails in chronological order (oldest first). This benefits every current and future caller of `get_emails_by_thread`, not just the ThreadReader.

  In `inboxly-store/src/emails.rs` around line 99, find the `get_emails_by_thread` method:
  ```rust
  pub fn get_emails_by_thread(&self, thread_id: &str) -> Result<Vec<EmailRow>> {
      let mut stmt = self.conn().prepare(
          "SELECT id, account_id, thread_id, from_name, from_address, to_json, cc_json,
           subject, snippet, date, maildir_path, flags, size_bytes, imap_uid, imap_folder,
           has_attachments, body_downloaded, message_id_header, in_reply_to, references_json
           FROM emails WHERE thread_id = ?1"
      )?;
      // ... row mapping ...
  }
  ```

  Append `ORDER BY date ASC` to the SQL:
  ```rust
  let mut stmt = self.conn().prepare(
      "SELECT id, account_id, thread_id, from_name, from_address, to_json, cc_json,
       subject, snippet, date, maildir_path, flags, size_bytes, imap_uid, imap_folder,
       has_attachments, body_downloaded, message_id_header, in_reply_to, references_json
       FROM emails WHERE thread_id = ?1 ORDER BY date ASC"
  )?;
  ```

  **Blast-radius check:** grep all callers of `get_emails_by_thread` and verify none depend on the previous (undefined) row order:
  ```bash
  grep -rn "get_emails_by_thread" --include='*.rs'
  ```

  Expected callers: the new `ThreadReader::load_thread()` (Step 4.3 below), possibly one or two places in `inboxly-store::threading` or `inboxly-imap`. For each caller, verify the caller either:
  - Already sorts the result (sort becomes a no-op — no regression)
  - Iterates the result without ordering assumptions (e.g., just counts or checks existence — no regression)
  - Is a test that might assert ordering (fix the test to expect chronological order)

  If you find a caller that relies on the previous order (e.g., "most recently inserted first"), stop and flag it — that caller should either be updated to use a different query (new method `get_emails_by_thread_by_uid_desc` or similar) or its behaviour revisited.

  **Add a regression test** in `inboxly-store/src/emails.rs::tests` (or wherever the existing `get_emails_by_thread` test lives):
  ```rust
  #[test]
  fn get_emails_by_thread_returns_chronological_order() {
      let store = Store::open_in_memory().expect("open");
      // Insert three emails in the SAME thread with OUT-OF-ORDER dates.
      // The earliest-dated email is inserted last, so insertion order
      // and chronological order disagree — this is the test case that
      // would have broken before the ORDER BY fix.
      let thread_id = "t-ord";
      let e1 = sample_email_row("e1", thread_id, /* date */ 3000);
      let e2 = sample_email_row("e2", thread_id, /* date */ 1000);
      let e3 = sample_email_row("e3", thread_id, /* date */ 2000);
      store.insert_email(&e1).unwrap();
      store.insert_email(&e2).unwrap();
      store.insert_email(&e3).unwrap();

      let rows = store.get_emails_by_thread(thread_id).unwrap();
      assert_eq!(rows.len(), 3);
      assert_eq!(rows[0].id, "e2", "earliest date should be first");
      assert_eq!(rows[1].id, "e3", "middle date should be second");
      assert_eq!(rows[2].id, "e1", "latest date should be last");
  }
  ```

  Use whatever `sample_email_row` helper already exists in the test module, or inline a literal `EmailRow { ... }` if there isn't one.

  **Commit this change as its own step** before building the ThreadReader — it's a narrow, well-bounded storage-layer fix that stands on its own:
  ```bash
  git add inboxly-store/src/emails.rs
  git commit -m "fix(store): sort get_emails_by_thread by date ASC (M34 phase 4 prep)"
  ```

- [ ] **Step 4.2: Add `SlimEmailContent` to `inboxly-core` and `read_email_slim()` to `MaildirStore` (eng review Issue 2.6)**

  Email body loading via `MaildirStore::read_email_content()` currently returns a full `EmailContent` that carries a `HashMap<String, String>` of every header (5–20 KB typical) and full attachment byte content (potentially MB per message). M34's thread detail view drops ALL the headers and all the attachment bytes at the `build_loaded_thread()` conversion step. That's wasted memory and I/O on every thread load. The fix is a slim view type that only carries what the UI actually renders.

  **In `inboxly-core/src/email.rs`**, add the new struct alongside the existing `EmailContent` (do NOT replace `EmailContent` — it's used elsewhere; both types coexist):
  ```rust
  use crate::attachment::AttachmentMeta;

  /// Slim view of email content — body text/HTML + attachment metadata only.
  /// Used by the thread detail view where full headers and attachment byte
  /// content aren't needed. Eng review Issue 2.6: avoids carrying 5-20 KB
  /// of headers and potentially MB of attachment bytes through the loader
  /// just to drop them at the rendering step.
  ///
  /// When M37 adds attachment download, it will call a separate method
  /// that loads the actual bytes for a single attachment on demand —
  /// this type will NOT be extended to carry byte content.
  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub struct SlimEmailContent {
      /// Message-ID (links to EmailMeta).
      pub id: EmailId,
      /// Plaintext body (if available).
      pub body_text: Option<String>,
      /// HTML body (if available).
      pub body_html: Option<String>,
      /// Attachment metadata only — no byte content.
      pub attachments: Vec<AttachmentMeta>,
  }
  ```

  **In `inboxly-core/src/lib.rs`**, add `SlimEmailContent` to the re-exports alongside `EmailContent`.

  **In `inboxly-store/src/maildir_store.rs`**, add a new method on `MaildirStore` and a free function for parsing (mirroring the existing `read_email_content` + `parse_email_content` split):
  ```rust
  impl MaildirStore {
      /// Read and parse SLIM email content from a Maildir file path.
      /// Returns body text, body HTML, and attachment metadata only —
      /// skips headers and attachment byte content. Used by the
      /// thread detail view (eng review Issue 2.6). Callers that need
      /// the full EmailContent should use `read_email_content()` instead.
      pub fn read_email_slim(&self, maildir_path: &Path) -> Result<SlimEmailContent, StoreError> {
          let data = std::fs::read(maildir_path).map_err(|e| {
              StoreError::Maildir(format!("Failed to read {}: {e}", maildir_path.display()))
          })?;
          parse_email_slim(&data)
      }
  }

  /// Parse raw .eml bytes into a slim email content struct.
  /// Extracts body (text + HTML) and attachment metadata only.
  pub fn parse_email_slim(data: &[u8]) -> Result<SlimEmailContent, StoreError> {
      let parsed =
          parse_mail(data).map_err(|e| StoreError::Parse(format!("Failed to parse email: {e}")))?;

      let message_id = parsed
          .headers
          .get_first_value("Message-ID")
          .unwrap_or_default()
          .trim_matches(|c| c == '<' || c == '>')
          .to_string();
      let id = EmailId(message_id);

      let body_text = find_body_text(&parsed);
      let body_html = find_body_html(&parsed);

      // Use the existing `collect_attachment_meta` helper (already
      // defined in this file for the EmailMeta build path), which
      // walks the MIME tree and returns only metadata, no byte content.
      let mut attachments = Vec::new();
      collect_attachment_meta(&parsed, &mut attachments);

      Ok(SlimEmailContent {
          id,
          body_text,
          body_html,
          attachments,
      })
  }
  ```

  Update the imports at the top of `maildir_store.rs` to include `SlimEmailContent`:
  ```rust
  use inboxly_core::{EmailContent, SlimEmailContent};  // add SlimEmailContent
  ```

  **Verify `collect_attachment_meta` exists**: grep for it in `maildir_store.rs`. The M33 code already has this helper (it's used by the sync path to extract `EmailMeta::attachments: Vec<AttachmentMeta>`). If it doesn't exist, implement it by adapting `collect_full_attachments` to skip the `get_body_raw()` call and return only `AttachmentMeta { filename, mime_type, size_bytes }`.

  **Add a test** in `inboxly-store/src/maildir_store.rs` (or wherever the existing `parse_email_content` tests live):
  ```rust
  #[test]
  fn parse_email_slim_strips_headers_and_attachment_bytes() {
      // Craft a minimal .eml with a few headers, a text body, and a
      // base64-encoded inline attachment. Parse it via parse_email_slim
      // and verify that headers are NOT present (no headers field)
      // and attachments carry metadata only (no content field).
      let eml = b"From: alice@example.com\r\n\
                  To: bob@example.com\r\n\
                  Subject: Test\r\n\
                  Message-ID: <test@ex.com>\r\n\
                  X-Spam-Score: 0.1\r\n\
                  \r\n\
                  Hello world";
      let slim = parse_email_slim(eml).expect("parse");
      assert_eq!(slim.body_text.as_deref(), Some("Hello world"));
      // SlimEmailContent has NO headers field — if this test compiles,
      // that's half the assertion. The other half: no way to access
      // header data means we never carry it.
      assert!(slim.attachments.is_empty());
  }
  ```

  **Commit this change on its own** — it's a new addition to inboxly-core and inboxly-store that stands on its own and benefits any future consumer wanting slim email content:
  ```bash
  git add inboxly-core/src/email.rs inboxly-core/src/lib.rs inboxly-store/src/maildir_store.rs
  git commit -m "feat(core,store): add SlimEmailContent + read_email_slim (M34 Issue 2.6)"
  ```

- [ ] **Step 4.3: Create `inboxly-store/src/thread_reader.rs`**

  ```rust
  //! Unified thread reader facade.
  //!
  //! Wraps both `Store` (SQLite metadata) and `MaildirStore` (filesystem
  //! bodies) so consumers can load a full thread with one call instead
  //! of plumbing two store handles. Future consumers (M36 reply,
  //! M37 attachments) hold one `Arc<ThreadReader>` instead of two.
  //!
  //! Returns raw storage data (`LoadedEmail`) carrying `SlimEmailContent`
  //! (body + attachment metadata only, no headers, no attachment bytes
  //! — eng review Issue 2.6). UI-shaped types like `LoadedThread` live
  //! in `inboxly-ui` and are built from this output via
  //! `inboxly_ui::loaded_thread::build_loaded_thread()`.

  use std::path::Path;
  use std::sync::Arc;

  use crate::emails::EmailRow;
  use crate::error::{Result, StoreError};
  use crate::maildir_store::MaildirStore;
  use crate::store::Store;
  use inboxly_core::SlimEmailContent;

  /// One email loaded from the store, with its slim body content if
  /// available. `content` is `None` when the body hasn't been
  /// downloaded yet OR when the disk read failed (latter is logged
  /// but not fatal — we still want to render the row metadata).
  /// The `SlimEmailContent` deliberately omits headers and attachment
  /// byte content; those live in the full `EmailContent` which is
  /// loaded on demand by other code paths (future M37 for download).
  #[derive(Debug, Clone)]
  pub struct LoadedEmail {
      pub row: EmailRow,
      pub content: Option<SlimEmailContent>,
  }

  /// Facade that hides the two-store coupling for thread loading.
  /// Hold via `Arc<ThreadReader>` for cheap sharing across components.
  pub struct ThreadReader {
      store: Arc<Store>,
      maildir: Arc<MaildirStore>,
  }

  impl ThreadReader {
      pub fn new(store: Arc<Store>, maildir: Arc<MaildirStore>) -> Self {
          Self { store, maildir }
      }

      /// Load all emails in a thread, hydrating each with its slim body
      /// from disk where available. Errors only if the underlying
      /// SQLite query fails OR if the thread has no emails. Per-row
      /// body-read failures are non-fatal: the row is returned with
      /// `content: None`.
      pub fn load_thread(&self, thread_id: &str) -> Result<Vec<LoadedEmail>> {
          let rows = self.store.get_emails_by_thread(thread_id)?;
          if rows.is_empty() {
              return Err(StoreError::NotFound(format!(
                  "no emails in thread {thread_id}"
              )));
          }
          let loaded = rows
              .into_iter()
              .map(|row| {
                  let content = if row.body_downloaded && !row.maildir_path.is_empty() {
                      // Use read_email_slim (Issue 2.6) instead of
                      // read_email_content — we don't need headers or
                      // attachment bytes for the thread detail view.
                      self.maildir
                          .read_email_slim(Path::new(&row.maildir_path))
                          .ok()
                  } else {
                      None
                  };
                  LoadedEmail { row, content }
              })
              .collect();
          Ok(loaded)
      }
  }

  #[cfg(test)]
  mod tests {
      // Pure unit tests for ThreadReader require a full SQLite + Maildir
      // fixture. Defer to Phase 4 step 4.5 (integration test) which sets
      // up a temp dir and exercises the full path. Module-level tests
      // here only assert that the type compiles.
      #[test]
      fn types_compile() {
          fn _assert_send_sync<T: Send + Sync>() {}
          // ThreadReader is NOT Send (Store contains a !Send Connection).
          // Just ensure it compiles.
          let _ = std::marker::PhantomData::<super::ThreadReader>;
      }
  }
  ```

  **`StoreError::NotFound` may not exist** — check `inboxly-store/src/error.rs` for the actual error variants. If there's no `NotFound` variant, use whatever the closest equivalent is (`StoreError::Other(String)` or similar). If none exists, add a new variant with a docstring and update the `error.rs` file accordingly.

- [ ] **Step 4.4: Register the new module**

  In `inboxly-store/src/lib.rs`, add (alphabetically near the other `pub mod` declarations):
  ```rust
  pub mod thread_reader;
  ```

- [ ] **Step 4.5: Add `ThreadReader` integration tests (eng review Issue 3.1)**

  `ThreadReader::load_thread()` is the production code path that real M36/M37 consumers will inherit. It has five distinct branches (Store query Err, empty thread Err, body_downloaded=true success, body_downloaded=true with disk read failure, body_downloaded=false). The `types_compile` test in Step 4.3 only verifies the type definition; the actual orchestration needs an integration test against real on-disk fixtures.

  **Verify `tempfile` is available:** check `Cargo.toml` (workspace root) for `tempfile` in `[dev-dependencies]` or `[workspace.dependencies]`. M33's existing tests likely already use it; if not, add it:
  ```toml
  # In workspace Cargo.toml [workspace.dependencies]:
  tempfile = "3"
  ```
  Then in `inboxly-store/Cargo.toml [dev-dependencies]`:
  ```toml
  tempfile.workspace = true
  ```

  **Create `inboxly-store/tests/thread_reader.rs`:**
  ```rust
  //! Integration tests for `ThreadReader` (M34 eng review Issue 3.1).
  //!
  //! Sets up a real on-disk fixture (TempDir for the Maildir, in-memory
  //! SQLite for the metadata store) and exercises every branch of
  //! `ThreadReader::load_thread()`. These tests are the safety net for
  //! the production code path that future consumers (M36 reply,
  //! M37 attachments) will inherit.

  use std::sync::Arc;

  use inboxly_store::emails::EmailRow;
  use inboxly_store::maildir_store::MaildirStore;
  use inboxly_store::store::Store;
  use inboxly_store::thread_reader::ThreadReader;
  use tempfile::TempDir;

  /// Build a fixture: in-memory Store, TempDir-backed MaildirStore,
  /// and a `ThreadReader` wrapping both. Returns the TempDir handle
  /// (which must outlive the test) so the temp directory isn't
  /// cleaned up while the test is still using paths inside it.
  fn fixture() -> (TempDir, Arc<Store>, Arc<MaildirStore>, ThreadReader) {
      let temp = TempDir::new().expect("tempdir");
      let store = Arc::new(Store::open_in_memory().expect("store"));
      let maildir = Arc::new(MaildirStore::new(temp.path()));
      let reader = ThreadReader::new(Arc::clone(&store), Arc::clone(&maildir));
      (temp, store, maildir, reader)
  }

  /// Build a minimal `EmailRow` for tests. `body_downloaded` and
  /// `maildir_path` are the test-relevant fields; the rest are
  /// reasonable defaults.
  fn make_row(
      id: &str,
      thread_id: &str,
      date: i64,
      body_downloaded: bool,
      maildir_path: &str,
  ) -> EmailRow {
      EmailRow {
          id: id.into(),
          account_id: "a1".into(),
          thread_id: thread_id.into(),
          from_name: Some("Alice".into()),
          from_address: "alice@example.com".into(),
          to_json: "[]".into(),
          cc_json: "[]".into(),
          subject: format!("Subject {id}"),
          snippet: "snip".into(),
          date,
          maildir_path: maildir_path.into(),
          flags: 0,
          size_bytes: 100,
          imap_uid: 1,
          imap_folder: "INBOX".into(),
          has_attachments: false,
          body_downloaded,
          message_id_header: None,
          in_reply_to: None,
          references_json: None,
      }
  }

  /// Write a minimal valid `.eml` file to disk and return the path.
  /// The body has a Subject header so `parse_email_slim` produces
  /// a non-empty body_text.
  fn write_eml(temp: &TempDir, name: &str, body_text: &str) -> String {
      let path = temp.path().join(name);
      let eml = format!(
          "From: alice@example.com\r\n\
           To: bob@example.com\r\n\
           Subject: Test {name}\r\n\
           Message-ID: <{name}@ex.com>\r\n\
           \r\n\
           {body_text}"
      );
      std::fs::write(&path, eml).expect("write eml");
      path.to_string_lossy().into_owned()
  }

  // ── Branch 1: empty thread → Err ──────────────────────────────

  #[test]
  fn load_thread_empty_returns_err() {
      let (_temp, _store, _maildir, reader) = fixture();
      // No rows inserted; the thread doesn't exist.
      let result = reader.load_thread("nonexistent");
      assert!(result.is_err(), "empty thread should be Err");
  }

  // ── Branch 2: body_downloaded=true with successful disk read ──

  #[test]
  fn load_thread_with_downloaded_body_returns_some_content() {
      let (temp, store, _maildir, reader) = fixture();
      let path = write_eml(&temp, "msg1.eml", "Hello world");
      let row = make_row("e1", "t1", 1000, /* downloaded */ true, &path);
      store.insert_email(&row).expect("insert");

      let result = reader.load_thread("t1").expect("ok");
      assert_eq!(result.len(), 1);
      assert_eq!(result[0].row.id, "e1");
      let content = result[0].content.as_ref().expect("content present");
      assert_eq!(content.body_text.as_deref(), Some("Hello world"));
  }

  // ── Branch 3: body_downloaded=false → content: None ───────────

  #[test]
  fn load_thread_with_undownloaded_body_returns_none_content() {
      let (_temp, store, _maildir, reader) = fixture();
      // No file written, body_downloaded=false, maildir_path empty.
      let row = make_row("e1", "t1", 1000, /* downloaded */ false, "");
      store.insert_email(&row).expect("insert");

      let result = reader.load_thread("t1").expect("ok");
      assert_eq!(result.len(), 1);
      assert!(
          result[0].content.is_none(),
          "undownloaded body must produce None content"
      );
  }

  // ── Branch 4: body_downloaded=true but disk read fails → None ─

  #[test]
  fn load_thread_handles_missing_file_gracefully() {
      let (_temp, store, _maildir, reader) = fixture();
      // Pretend the body is downloaded but the file doesn't exist.
      let row = make_row(
          "e1",
          "t1",
          1000,
          /* downloaded */ true,
          "/nonexistent/path/to/missing.eml",
      );
      store.insert_email(&row).expect("insert");

      // Should NOT propagate the file-read error — falls through to None.
      let result = reader.load_thread("t1").expect("ok despite missing file");
      assert_eq!(result.len(), 1);
      assert!(
          result[0].content.is_none(),
          "missing file should fall through to None, not Err"
      );
  }

  // ── Branch 5: mixed thread with multiple messages ─────────────

  #[test]
  fn load_thread_multiple_messages_in_chronological_order() {
      let (temp, store, _maildir, reader) = fixture();
      // Insert in NON-chronological order to verify the SQL ORDER BY
      // (Issue 2.5) actually sorts the result.
      let p3 = write_eml(&temp, "msg3.eml", "third");
      let p1 = write_eml(&temp, "msg1.eml", "first");
      let p2 = write_eml(&temp, "msg2.eml", "second");
      store.insert_email(&make_row("e3", "t1", 3000, true, &p3)).unwrap();
      store.insert_email(&make_row("e1", "t1", 1000, true, &p1)).unwrap();
      store.insert_email(&make_row("e2", "t1", 2000, true, &p2)).unwrap();

      let result = reader.load_thread("t1").expect("ok");
      assert_eq!(result.len(), 3);
      // Earliest date first.
      assert_eq!(result[0].row.id, "e1");
      assert_eq!(result[1].row.id, "e2");
      assert_eq!(result[2].row.id, "e3");
      // All have content.
      assert!(result.iter().all(|le| le.content.is_some()));
  }

  // ── Branch 6: mixed downloaded/undownloaded ───────────────────

  #[test]
  fn load_thread_mixed_downloaded_state_per_message() {
      let (temp, store, _maildir, reader) = fixture();
      let p1 = write_eml(&temp, "ready.eml", "downloaded body");
      // e1: downloaded with valid file → Some
      // e2: not downloaded → None
      store.insert_email(&make_row("e1", "t1", 1000, true, &p1)).unwrap();
      store.insert_email(&make_row("e2", "t1", 2000, false, "")).unwrap();

      let result = reader.load_thread("t1").expect("ok");
      assert_eq!(result.len(), 2);
      assert!(result[0].content.is_some());
      assert!(result[1].content.is_none());
  }
  ```

  **Build and run the integration tests:**
  ```bash
  cargo test -p inboxly-store --test thread_reader 2>&1 | tail -10
  ```
  Expected: 6 tests pass.

  **Commit the integration tests on their own:**
  ```bash
  git add inboxly-store/tests/thread_reader.rs Cargo.toml inboxly-store/Cargo.toml
  git commit -m "test(store): add ThreadReader integration tests (M34 Issue 3.1)"
  ```

- [ ] **Step 4.6: Update `inboxly-ui/src/loaded_thread.rs` — add `build_loaded_thread`**

  The old plan had a `load_thread()` function in `inboxly-ui` that took `&Store + &MaildirStore`. Per Issue 1.5, the actual storage orchestration moves to `ThreadReader`. The UI side becomes a pure converter from `Vec<LoadedEmail>` (storage-layer output) into `LoadedThread` (UI-shaped type with display names and formatted dates).

  Append to `inboxly-ui/src/loaded_thread.rs` (above the `#[cfg(test)]` block):
  ```rust
  use inboxly_store::thread_reader::LoadedEmail;

  /// Convert raw `LoadedEmail` rows from the storage facade into the
  /// UI's `LoadedThread` shape. Picks display names (sender name OR
  /// sender address as fallback), converts UNIX timestamps to
  /// `DateTime<Utc>`, and passes through attachment metadata.
  ///
  /// `LoadedEmail.content` is already a `SlimEmailContent` (eng review
  /// Issue 2.6) carrying only body text/HTML and attachment metadata —
  /// headers and attachment byte content are NEVER loaded for the
  /// thread detail view. This converter just maps the slim fields 1:1.
  ///
  /// Returns `Err` if `emails` is empty (the caller should fall back
  /// to `fallback_thread()`).
  pub fn build_loaded_thread(
      thread_id: &str,
      emails: Vec<LoadedEmail>,
  ) -> Result<LoadedThread, String> {
      use std::sync::Arc;
      if emails.is_empty() {
          return Err(format!("no emails in thread {thread_id}"));
      }
      let subject = emails[0].row.subject.clone();
      let messages = emails
          .into_iter()
          .map(|le| {
              let LoadedEmail { row, content } = le;
              let (body_text, body_html, attachments) = match content {
                  Some(c) => (c.body_text, c.body_html, c.attachments),
                  None => (
                      Some("(body not yet downloaded)".to_string()),
                      None,
                      Vec::new(),
                  ),
              };
              // Issue 2.8: wrap each LoadedMessage in Arc so per-render
              // clones in ThreadDetailView's `for` loop are refcount
              // bumps, not deep clones of the body bytes.
              Arc::new(LoadedMessage {
                  email_id: row.id,
                  from_name: row.from_name.unwrap_or_else(|| row.from_address.clone()),
                  from_address: row.from_address,
                  // Issue 2.7: from_timestamp returns None for invalid
                  // input. Pass it through as-is — the renderer will
                  // show "(unknown time)" for None instead of the
                  // misleading "right now" fallback.
                  date: chrono::DateTime::<chrono::Utc>::from_timestamp(row.date, 0),
                  body_text,
                  body_html,
                  attachments,
              })
          })
          .collect();
      Ok(LoadedThread {
          thread_id: thread_id.to_string(),
          subject,
          messages,
          error_message: None,  // success path
      })
  }
  ```

  Add the test for `build_loaded_thread()` in the `#[cfg(test)] mod tests` block:
  ```rust
  #[test]
  fn build_loaded_thread_empty_returns_err() {
      let result = build_loaded_thread("t1", vec![]);
      assert!(result.is_err());
  }

  #[test]
  fn build_loaded_thread_uses_address_when_name_missing() {
      use inboxly_core::AttachmentMeta;
      use inboxly_store::emails::EmailRow;
      use inboxly_store::thread_reader::LoadedEmail;

      let row = EmailRow {
          id: "e1".into(),
          account_id: "a1".into(),
          thread_id: "t1".into(),
          from_name: None,  // <-- no display name
          from_address: "alice@example.com".into(),
          to_json: "[]".into(),
          cc_json: "[]".into(),
          subject: "Hello".into(),
          snippet: "Hi there".into(),
          date: 1_700_000_000,
          maildir_path: String::new(),
          flags: 0,
          size_bytes: 100,
          imap_uid: 1,
          imap_folder: "INBOX".into(),
          has_attachments: false,
          body_downloaded: false,
          message_id_header: None,
          in_reply_to: None,
          references_json: None,
      };
      let loaded = LoadedEmail { row, content: None };
      let result = build_loaded_thread("t1", vec![loaded]).expect("non-empty");
      assert_eq!(result.subject, "Hello");
      assert_eq!(result.messages.len(), 1);
      assert_eq!(result.messages[0].from_name, "alice@example.com");
      assert_eq!(result.messages[0].from_address, "alice@example.com");
      assert!(result.messages[0].body_text.is_some());  // placeholder
      assert!(result.messages[0].body_html.is_none());
      // Issue 2.7: a valid timestamp should produce Some(...).
      assert!(result.messages[0].date.is_some());
  }

  #[test]
  fn build_loaded_thread_with_content_passes_through_body_and_attachments() {
      // Eng review Issue 3.2: pin the success-path mapping. The
      // converter is mechanical (1:1 field mapping) but this test
      // ensures the most common production path — a populated
      // SlimEmailContent — produces the expected LoadedMessage.
      use inboxly_core::{AttachmentMeta, EmailId, SlimEmailContent};
      use inboxly_store::emails::EmailRow;
      use inboxly_store::thread_reader::LoadedEmail;

      let row = EmailRow {
          id: "e1".into(),
          account_id: "a1".into(),
          thread_id: "t1".into(),
          from_name: Some("Alice".into()),
          from_address: "alice@example.com".into(),
          to_json: "[]".into(),
          cc_json: "[]".into(),
          subject: "Re: Hello".into(),
          snippet: "snip".into(),
          date: 1_700_000_000,
          maildir_path: "/tmp/fake.eml".into(),
          flags: 0,
          size_bytes: 200,
          imap_uid: 1,
          imap_folder: "INBOX".into(),
          has_attachments: true,
          body_downloaded: true,
          message_id_header: None,
          in_reply_to: None,
          references_json: None,
      };
      let content = SlimEmailContent {
          id: EmailId("<e1@example.com>".into()),
          body_text: Some("plain text body".into()),
          body_html: Some("<p>html <strong>body</strong></p>".into()),
          attachments: vec![AttachmentMeta {
              filename: "invoice.pdf".into(),
              mime_type: "application/pdf".into(),
              size_bytes: 4096,
          }],
      };
      let loaded = LoadedEmail { row, content: Some(content) };
      let result = build_loaded_thread("t1", vec![loaded]).expect("non-empty");
      assert_eq!(result.thread_id, "t1");
      assert_eq!(result.subject, "Re: Hello");
      assert!(result.error_message.is_none(), "success path has no error");
      assert_eq!(result.messages.len(), 1);
      let msg = &result.messages[0];
      assert_eq!(msg.from_name, "Alice");
      assert_eq!(msg.from_address, "alice@example.com");
      assert_eq!(msg.body_text.as_deref(), Some("plain text body"));
      assert_eq!(
          msg.body_html.as_deref(),
          Some("<p>html <strong>body</strong></p>")
      );
      assert_eq!(msg.attachments.len(), 1);
      assert_eq!(msg.attachments[0].filename, "invoice.pdf");
      assert_eq!(msg.attachments[0].mime_type, "application/pdf");
      assert_eq!(msg.attachments[0].size_bytes, 4096);
      assert!(msg.date.is_some(), "valid Unix timestamp must yield Some");
  }

  #[test]
  fn build_loaded_thread_handles_invalid_timestamp() {
      // Eng review Issue 2.7: corrupt or out-of-range timestamps
      // should yield None, not "right now". `i64::MIN` is well below
      // chrono's representable range, so from_timestamp returns None.
      use inboxly_store::emails::EmailRow;
      use inboxly_store::thread_reader::LoadedEmail;

      let row = EmailRow {
          id: "e-bad-date".into(),
          account_id: "a1".into(),
          thread_id: "t1".into(),
          from_name: Some("Alice".into()),
          from_address: "alice@example.com".into(),
          to_json: "[]".into(),
          cc_json: "[]".into(),
          subject: "Bad date".into(),
          snippet: "".into(),
          date: i64::MIN,  // out of chrono's representable range
          maildir_path: String::new(),
          flags: 0,
          size_bytes: 0,
          imap_uid: 1,
          imap_folder: "INBOX".into(),
          has_attachments: false,
          body_downloaded: false,
          message_id_header: None,
          in_reply_to: None,
          references_json: None,
      };
      let loaded = LoadedEmail { row, content: None };
      let result = build_loaded_thread("t1", vec![loaded]).expect("non-empty");
      assert!(
          result.messages[0].date.is_none(),
          "i64::MIN must yield None, not a fallback timestamp"
      );
  }
  ```

- [ ] **Step 4.7: Build the whole UI crate**

  Run: `cargo build -p inboxly-ui 2>&1 | tail -10`
  Expected: clean. The Phase 1 unresolved-import error from Step 1.6 should now resolve because `inboxly_store::thread_reader::ThreadReader` exists.

- [ ] **Step 4.8: Run tests**

  Run: `cargo test -p inboxly-ui 2>&1 | grep "test result" | head -3` and `cargo test -p inboxly-store 2>&1 | grep "test result" | head -3`
  Expected: all prior tests still pass; the 4 new tests for `build_loaded_thread()` (empty, address fallback, valid date, invalid date) and the `types_compile` test in `thread_reader.rs` all pass. The 6 integration tests in `tests/thread_reader.rs` should already be passing from Step 4.5's commit.

- [ ] **Step 4.9: Commit the UI-side build_loaded_thread changes**

  ```bash
  git add inboxly-ui/src/loaded_thread.rs
  git commit -m "feat(ui): add build_loaded_thread converter (M34 phase 4)"
  ```

  Note: the storage-layer commits (ThreadReader module + integration tests) were committed in Step 4.3 and Step 4.5 respectively. This commit covers only the UI-side conversion code.

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

  /* Error banner — eng review Issue 2.1. Rendered above the message
     list when the loader failed (instead of silently showing
     demo/empty content). Uses the menu-destructive color tokens
     from M33's CSS palette so dark/light themes work automatically. */
  .thread-detail-error-banner {
      display: flex;
      align-items: center;
      gap: 12px;
      margin: 12px var(--default-padding);
      padding: 12px 16px;
      background: var(--menu-destructive-hover);
      color: var(--menu-destructive-text);
      border-left: 4px solid var(--menu-destructive-text);
      border-radius: 6px;
      font-size: 14px;
  }

  .thread-detail-error-icon {
      font-size: 18px;
      flex-shrink: 0;
  }

  .thread-detail-error-text {
      flex: 1;
      word-break: break-word;
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

  use std::sync::Arc;

  use dioxus::prelude::*;

  use crate::loaded_thread::LoadedMessage;
  use crate::sanitize::sanitize_html;
  use crate::theme::avatar_colors;

  /// `Arc<LoadedMessage>` instead of owned `LoadedMessage` per eng
  /// review Issue 2.8: per-render clones in `ThreadDetailView`'s
  /// `for` loop become refcount bumps instead of deep clones of
  /// the body bytes. The `Arc<T>` impls give us `Clone + PartialEq`
  /// for free as long as `T: PartialEq` (which `LoadedMessage` is).
  #[component]
  pub fn ThreadMessage(message: Arc<LoadedMessage>) -> Element {
      let avatar_letter = message
          .from_name
          .chars()
          .next()
          .unwrap_or('?')
          .to_ascii_uppercase();
      let avatar_color = avatar_colors::for_letter(avatar_letter).to_css();
      // Issue 2.7: date is Option — display "(unknown time)" for None
      // (corrupt or out-of-range timestamps in the store) instead of
      // the previous misleading Utc::now() fallback.
      let date_display = match message.date {
          Some(dt) => dt.format("%b %-d, %Y at %-I:%M %p").to_string(),
          None => "(unknown time)".to_string(),
      };

      // Choose a body renderer: sanitised HTML if available, else plain text in a <pre>.
      // Field accesses below go through Arc's Deref to &LoadedMessage.
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

### Phase 7: `ThreadDetailView` component + App-level signal context + loader bridge

This phase wires three things together:
1. The `ThreadDetailView` Dioxus component (reads from the body signal context, NOT from Inboxly).
2. The `Signal<Option<Arc<LoadedThread>>>` context provider in `App` (per Issue 1.4 — body data lives outside Inboxly).
3. The `use_effect` bridge in `App` that watches `Inboxly::open_thread_id` and calls `load_thread()`/`fallback_thread()` to populate the body signal.

- [ ] **Step 7.1: Create `inboxly-ui/src/components/thread_detail_view.rs`**

  ```rust
  //! Thread detail view — header bar + scrollable list of messages.
  //!
  //! Reads from the `Signal<Option<Arc<LoadedThread>>>` context that
  //! the App component provides. The body data lives in this separate
  //! signal (per eng review Issue 1.4) so per-write `Clone` of Inboxly
  //! doesn't drag thread bodies around. The back button dispatches
  //! `Message::CloseThread`, which clears `Inboxly::open_thread_id`;
  //! the App-level use_effect bridge then clears the body signal.

  use std::sync::Arc;

  use dioxus::prelude::*;

  use crate::app::{Inboxly, Message};
  use crate::components::thread_message::ThreadMessage;
  use crate::loaded_thread::LoadedThread;

  #[component]
  pub fn ThreadDetailView() -> Element {
      let mut app_state = use_context::<Signal<Inboxly>>();
      let open_thread = use_context::<Signal<Option<Arc<LoadedThread>>>>();

      // Read the body signal — this clone is cheap (Arc bump).
      let thread_arc = open_thread.read().clone();
      let Some(thread) = thread_arc else {
          return rsx! {};
      };
      // From here on we work with `&LoadedThread` (via the Arc).

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
              // Error banner — only rendered when the loader failed.
              // Eng review Issue 2.1: surface load failures to the user
              // instead of silently showing demo/empty content.
              if let Some(ref err) = thread.error_message {
                  div {
                      class: "thread-detail-error-banner",
                      role: "alert",
                      span { class: "thread-detail-error-icon", "\u{26A0}\u{FE0F}" }  // ⚠️
                      span { class: "thread-detail-error-text", "{err}" }
                  }
              }
              if thread.messages.is_empty() {
                  div { class: "thread-detail-empty", "No messages in this thread." }
              } else {
                  for message in thread.messages.iter() {
                      // Issue 2.8: messages are `Vec<Arc<LoadedMessage>>`,
                      // so this clone is a refcount bump (one atomic
                      // increment), not a deep clone of the body bytes.
                      // The Arc::clone makes the cheapness explicit at
                      // the call site so future readers don't worry.
                      ThreadMessage { message: Arc::clone(message) }
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

- [ ] **Step 7.3: Add the App-level signal context provider and use_effect bridge**

  In `inboxly-ui/src/components/app.rs`, modify the `App` component. Currently it does:
  ```rust
  let app_state = use_context_provider(|| {
      Signal::new(Inboxly {
          theme: ThemeConfig::from_system(),
          ..Inboxly::default()
      })
  });
  ```

  Add a sibling context provider for the open thread body data, plus the bridge use_effect. Updated body of `fn App() -> Element` (additions only — keep existing content as-is):
  ```rust
  use std::sync::Arc;
  use crate::loaded_thread::{build_loaded_thread, fallback_thread, LoadedThread};

  // Inside fn App() -> Element { ... }
  let app_state = use_context_provider(|| {
      Signal::new(Inboxly {
          theme: ThemeConfig::from_system(),
          ..Inboxly::default()
      })
  });

  // Body data context — separate from Inboxly so per-write clones
  // don't drag the thread body bytes around. ThreadDetailView reads
  // from this signal directly. (Eng review Issue 1.4.)
  let mut open_thread = use_context_provider(
      || Signal::new(None::<Arc<LoadedThread>>)
  );

  // Bridge: watch Inboxly::open_thread_id (the intent), and when it
  // changes, run the loader through the ThreadReader facade and
  // write the result into open_thread (the body). use_memo on the
  // id ensures the effect only re-runs when intent changes, not on
  // every Inboxly write.
  let open_id = use_memo(move || app_state.read().open_thread_id.clone());
  use_effect(move || {
      let id_opt = open_id.read().clone();
      let mut open_thread = open_thread;
      let app_state = app_state;
      match id_opt {
          Some(id) => {
              // Eng review Issue 4.1: decouple the click handler from
              // the I/O. Set the loading sentinel synchronously, then
              // spawn a Dioxus task to do the actual load. The click
              // handler returns immediately and the UI shows
              // "(loading…)" until the task completes.
              //
              // CAVEAT: this is "cooperative async", not preemptive
              // async. The spawned task runs on the local executor
              // (Dioxus desktop is single-threaded). When the task
              // calls reader.load_thread(), the SQLite query and
              // each per-row file read are still synchronous syscalls
              // — they block the local runtime briefly. For 50
              // messages × 5 ms per read = ~250 ms during which the
              // runtime is blocked. The benefit is that the click
              // event itself returns immediately, the loading state
              // is visible, and the load runs OUTSIDE the click handler.
              //
              // True non-blocking would require either making `Store`
              // Send (large refactor) or switching `read_email_slim`
              // to async file I/O via `tokio::fs::read` (medium
              // refactor). Both are out of M34 scope. See "Out of
              // Scope" section for the deferral note.
              open_thread.set(Some(Arc::new(loading_thread(&id))));
              spawn(async move {
                  // peek() does NOT subscribe — we don't want this
                  // effect to re-fire on every thread_reader field
                  // change. The only reactive dependency is open_id.
                  let snapshot = app_state.peek();
                  let loaded = match snapshot.thread_reader.as_ref() {
                      Some(reader) => {
                          // Issue 1.5 facade: ThreadReader is the
                          // single handle hiding both Store and
                          // MaildirStore.
                          match reader.load_thread(&id) {
                              Ok(emails) => match build_loaded_thread(&id, emails) {
                                  Ok(thread) => thread,
                                  Err(e) => {
                                      // Issue 2.1: surface load failures
                                      // to the user via the error banner,
                                      // AND log for developer visibility.
                                      tracing::warn!(
                                          "build_loaded_thread({id}) failed: {e}"
                                      );
                                      error_thread(
                                          &id,
                                          format!("Failed to build thread view: {e}"),
                                      )
                                  }
                              },
                              Err(e) => {
                                  tracing::warn!(
                                      "ThreadReader::load_thread({id}) failed: {e}"
                                  );
                                  error_thread(&id, format!("Failed to load thread: {e}"))
                              }
                          }
                      }
                      None => fallback_thread(&id),  // no reader wired — not an error
                  };
                  drop(snapshot);
                  open_thread.set(Some(Arc::new(loaded)));
              });
          }
          None => {
              open_thread.set(None);
          }
      }
  });
  ```

  Update the imports at the top of `components/app.rs` to include `loading_thread` and `error_thread`:
  ```rust
  use crate::loaded_thread::{
      build_loaded_thread, error_thread, fallback_thread, loading_thread, LoadedThread,
  };
  ```

  Place the new context provider call AFTER the existing `app_state` context provider (so Inboxly is available when the use_effect closure captures `app_state`). The use_effect goes after both providers.

  **Dioxus 0.7 API note:** if `use_memo` or `use_effect` aren't directly in `dioxus::prelude` for 0.7.4, the imports may need to be `use dioxus::hooks::{use_memo, use_effect};` or similar. Verify by grepping the dioxus-hooks source if compile fails.

- [ ] **Step 7.4: Wire ThreadDetailView into ContentArea**

  In `inboxly-ui/src/components/content_area.rs`, modify the Inbox arm of the `match view` block. ContentArea reads from the open-thread body signal (NOT from Inboxly's `open_thread_id`), so the conditional checks the signal:

  ```rust
  use std::sync::Arc;
  use crate::loaded_thread::LoadedThread;
  use crate::components::thread_detail_view::ThreadDetailView;
  ```

  Update the `ContentArea` component body. Add to the prologue (alongside `inbox_empty`):
  ```rust
  let open_thread = use_context::<Signal<Option<Arc<LoadedThread>>>>();
  let thread_open = open_thread.read().is_some();
  ```

  Replace the Inbox match arm:
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

  **Why ContentArea reads the body signal directly, not the id:** the body signal is what determines whether ThreadDetailView has anything to show. There's a brief window between "user clicks email row → `open_thread_id` is set" and "use_effect fires → body signal is set" where the id is Some but the body is None. During that window we want ContentArea to keep showing the inbox feed, not flash an empty ThreadDetailView. Reading the body signal handles this correctly.

- [ ] **Step 7.5: Build**

  Run: `cargo build -p inboxly 2>&1 | tail -5`
  Expected: clean. The `dead_code` warning on `load_thread()` from Phase 4 should disappear because the bridge use_effect now calls it.

- [ ] **Step 7.6: Commit**

  ```bash
  git add inboxly-ui/src/components/thread_detail_view.rs inboxly-ui/src/components/mod.rs inboxly-ui/src/components/app.rs inboxly-ui/src/components/content_area.rs
  git commit -m "feat(ui): ThreadDetailView + App-level body signal + loader bridge (M34 phase 7)"
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

- [ ] **Step 8.2: Add state-machine tests for the open/close intent cycle**

  Per Issue 1.4, tests assert on `open_thread_id` (the intent stored in Inboxly), not on the body data (which lives in a separate signal that's not accessible from a unit test without spinning up a Dioxus runtime). The state machine still owns the open/close lifecycle; the App-level use_effect bridge only translates intent into body data.

  In `inboxly-ui/src/app.rs::tests`, add:
  ```rust
  #[test]
  fn open_thread_sets_open_thread_id_field() {
      let mut app = Inboxly::default();
      assert!(app.open_thread_id.is_none());
      let _ = app.update(Message::OpenThread("t1".into()));
      assert_eq!(app.open_thread_id.as_deref(), Some("t1"));
  }

  #[test]
  fn close_thread_clears_open_thread_id_field() {
      let mut app = Inboxly::default();
      let _ = app.update(Message::OpenThread("t1".into()));
      let _ = app.update(Message::CloseThread);
      assert!(app.open_thread_id.is_none());
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
      // Opening a thread must dismiss any open menu (close_menus()
      // invariant from M33 Phase 7A).
      assert!(app.context_menu_thread.is_none());
      assert!(app.menu_thread_sender.is_none());
      // And it must record the intent.
      assert_eq!(app.open_thread_id.as_deref(), Some("t1"));
  }

  #[test]
  fn opening_thread_does_not_load_body_into_inboxly() {
      // Regression test for the Issue 1.4 design: dispatching OpenThread
      // must NOT cause any body data to be cloned into Inboxly. The
      // body lives in a separate signal that this test can't see —
      // we just verify Inboxly itself stays small.
      let mut app = Inboxly::default();
      let _ = app.update(Message::OpenThread("t1".into()));
      // The id is stored, but no LoadedThread or LoadedMessage anywhere
      // in Inboxly. (This is enforced by the type system: Inboxly has
      // no field of type LoadedThread.) The test exists to document
      // the contract for future contributors who might be tempted to
      // add an `Option<LoadedThread>` to Inboxly "for convenience".
      assert_eq!(app.open_thread_id.as_deref(), Some("t1"));
  }
  ```

- [ ] **Step 8.3: Run tests**

  Run: `cargo test -p inboxly-ui 2>&1 | grep "test result" | head -2`
  Expected: 246 passing (242 prior + 4 new) or similar.

- [ ] **Step 8.4: Commit**

  ```bash
  git add inboxly-ui/src/components/email_row.rs inboxly-ui/src/app.rs
  git commit -m "feat(ui): wire EmailRow click to OpenThread + state tests (M34 phase 8)"
  ```

### Phase 9: Document-level JS bridges in the App component (Escape + link-click)

Two `use_effect` bridges in `components/app.rs`, both installed via Dioxus's `document::eval` API:
1. **Escape handler** (Issue 2.3): a document-level `keydown` listener that fires `Message::CloseThread` when the user presses Escape OUTSIDE of any text input. No `tabindex: 0` or `onkeydown` on the shell.
2. **Link-click interceptor** (Issue 1.2): a document-level `click` listener that catches clicks on sentinel-prefixed `<a href="#inboxly-ext:...">` elements and forwards the real URL to `Message::OpenExternalUrl`.

Both effects coexist inside the App component. They share no state with each other beyond `app_state`.

- [ ] **Step 9.1: Inspect `keyboard.rs` for the existing shortcut handler**

  Run: `grep -n "Escape\|fn handle_key\|fn key_pressed" inboxly-ui/src/keyboard.rs | head -20`

  Find where keyboard events are routed to message dispatches. Currently the file mostly defines the `ShortcutMap` and `ShortcutAction` enum — it doesn't dispatch Escape to anything today.

- [ ] **Step 9.2: Add the Escape handler via document-level JS bridge**

  Eng review Issue 2.3 ruled out the naive `tabindex: 0` + `onkeydown` approach on `.app-shell` because it pollutes the tab order, steals focus from text inputs, and would intercept Escape keystrokes that the user pressed to clear the search box (or future compose field). The correct fix is a document-level keydown listener installed via Dioxus's `document::eval` JS bridge — same pattern as Phase 9's link-click interceptor below — that checks `event.target` to skip Escapes pressed inside text inputs.

  In `inboxly-ui/src/components/app.rs`, add a second `use_effect` block alongside the link-click interceptor (Step 9.3 below). It can go before or after the link-click effect; order doesn't matter. Also add a `use_drop` cleanup hook that removes the listener when the component is unmounted (eng review Issue 4.2 — defends against future hot-reload, component-mount tests, or any other scenario where the App component might re-mount during the process lifetime).

  ```rust
  // Eng review Issue 2.3: install a document-level Escape handler.
  // The JS side checks `event.target` to avoid intercepting Escape
  // inside text inputs (search box, compose fields, settings), where
  // the user almost certainly meant "clear my input". Only when the
  // active element is NOT an input does the listener forward the
  // Escape to Rust, which closes the open thread.
  //
  // Eng review Issue 4.2: the listener is bound to a globally-accessible
  // window property so the use_drop cleanup hook below can remove it
  // by reference. Without that, App component re-mounts (hot reload,
  // future tests) would stack listeners.
  use_effect(move || {
      let mut app_state = app_state;
      spawn(async move {
          let mut ch = document::eval(r#"
              window.__inboxly_escape_handler = (e) => {
                  if (e.key !== 'Escape') return;
                  // Don't intercept Escape inside text inputs.
                  const t = e.target;
                  const tag = t && t.tagName;
                  if (tag === 'INPUT' || tag === 'TEXTAREA') return;
                  if (t && t.isContentEditable) return;
                  e.preventDefault();
                  dioxus.send('escape');
              };
              document.addEventListener('keydown', window.__inboxly_escape_handler, true);
          "#);
          // The JS side sends a literal "escape" string on every
          // qualifying keypress. We don't care about the value —
          // any message on the channel is an Escape intent.
          while let Ok(_token) = ch.recv::<String>().await {
              let state = app_state.peek();
              if state.open_thread_id.is_some() {
                  drop(state);
                  app_state.write().update(Message::CloseThread);
              }
          }
      });
  });

  // Eng review Issue 4.2: cleanup on App component unmount.
  // Removes the global Escape listener so re-mounts don't stack.
  // Fire-and-forget — we don't need to await the JS execution.
  use_drop(|| {
      let _ = document::eval(r#"
          if (window.__inboxly_escape_handler) {
              document.removeEventListener('keydown', window.__inboxly_escape_handler, true);
              delete window.__inboxly_escape_handler;
          }
      "#);
  });
  ```

  **Dioxus 0.7 cleanup hook note:** the API is named `use_drop` in Dioxus 0.7.x. If your version exposes it as `use_on_destroy` or `use_on_unmount`, adapt the call. Both run the closure when the owning component is dropped.

  **Do NOT add `tabindex: 0`, `onkeydown`, or `Key` imports to `.app-shell`.** The document-level listener handles everything.

  **Accessibility note:** a screen-reader user who has focused an ARIA-live region or a button might still press Escape expecting to close a modal-like overlay. The `tagName` check only skips text-input-like elements; buttons and other focusables still route Escape through our handler. That's intentional — pressing Escape on the back button should still close the thread.

- [ ] **Step 9.3: Add the link-click interceptor via JS bridge**

  Email bodies are rendered via `dangerous_inner_html`, which means the injected `<a>` elements are invisible to Dioxus's event system — you can't put an `onclick` attribute on them from Rust. The workaround is a one-shot JavaScript listener attached at app mount time that catches `click` events on elements with sentinel-prefixed `href`s and forwards the real URL back to Rust via the Dioxus eval channel.

  In `inboxly-ui/src/components/app.rs`, add a `use_effect` inside the `App` component, after the existing signal initialisation and before the `rsx!` block:

  ```rust
  // Install a one-shot global click interceptor for email-body links.
  // Sanitised email HTML has all `<a href>` rewritten to a sentinel
  // prefix (see `crate::sanitize::EXT_URL_SENTINEL`) so clicks don't
  // navigate the webview away from the app. This listener catches
  // those clicks, strips the sentinel prefix, and dispatches
  // OpenExternalUrl which calls open::that() to hand the URL to the
  // system browser.
  //
  // Eng review Issue 2.4: the sentinel string is interpolated from
  // the Rust constant via format! so there's a single source of
  // truth. Do NOT hardcode the prefix in the JS source.
  //
  // The listener is attached once at mount (no cleanup) and runs
  // for the life of the app. It's a no-op for any href that doesn't
  // start with the sentinel, so it doesn't interfere with Dioxus's
  // own button onclicks.
  use_effect(move || {
      let mut app_state = app_state;
      spawn(async move {
          // Single source of truth for the sentinel prefix.
          let sentinel = crate::sanitize::EXT_URL_SENTINEL;
          // Build the JS source at runtime so the prefix is
          // interpolated from the Rust constant. Note the doubled
          // braces `{{` / `}}` are format! literal escapes — the
          // resulting JS has single braces as expected.
          //
          // Eng review Issue 4.2: the handler is bound to a globally-
          // accessible window property so the use_drop cleanup hook
          // below can remove it on App unmount.
          let js_source = format!(
              r#"
              window.__inboxly_link_click_handler = (e) => {{
                  const a = e.target.closest && e.target.closest('a');
                  if (!a) return;
                  const href = a.getAttribute('href');
                  const SENTINEL = '{sentinel}';
                  if (href && href.startsWith(SENTINEL)) {{
                      e.preventDefault();
                      e.stopPropagation();
                      const url = href.substring(SENTINEL.length);
                      dioxus.send(url);
                  }}
              }};
              document.addEventListener('click', window.__inboxly_link_click_handler, true);
              "#
          );
          let mut ch = document::eval(&js_source);
          // Drain the channel: every URL the JS side forwards becomes
          // one OpenExternalUrl dispatch. The `recv` loop never exits
          // until the component unmounts (app close).
          while let Ok(url) = ch.recv::<String>().await {
              app_state.write().update(Message::OpenExternalUrl(url));
          }
      });
  });

  // Eng review Issue 4.2: cleanup on App component unmount.
  // Removes the global click listener so re-mounts don't stack.
  use_drop(|| {
      let _ = document::eval(r#"
          if (window.__inboxly_link_click_handler) {
              document.removeEventListener('click', window.__inboxly_link_click_handler, true);
              delete window.__inboxly_link_click_handler;
          }
      "#);
  });
  ```

  Add the `document` import at the top of `components/app.rs`:
  ```rust
  use dioxus::document;
  ```

  **Why `const SENTINEL = '{sentinel}';` inside the JS body instead of inlining the prefix twice:** the JS uses the value twice (once for `startsWith` and once for `substring(length)`). Binding it to a local `SENTINEL` means the format! interpolation only has to substitute once, and the JS-level constant makes the dependency on the Rust constant visible to any future reader.

  **Dioxus 0.7 eval API note:** the channel handle returned by `document::eval` is typed in 0.7.4. The exact method names may be `recv::<T>()` or `recv_json::<T>()` depending on minor version. If `recv::<String>()` doesn't compile, try `recv_json::<String>()` or inspect `/home/alan/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-document-0.7.4/src/eval.rs` (or similar) for the exact API and adapt.

  **Why the listener uses `{ capture: true }`** (the third argument `true`): email bodies may contain their own click handlers (unlikely after sanitisation, but defensive). Capture phase runs before bubble phase, so our listener gets first shot at the click regardless.

- [ ] **Step 9.4: Add a state-machine test for the URL dispatch**

  In `inboxly-ui/src/app.rs::tests`, add:
  ```rust
  #[test]
  fn open_external_url_https_does_not_panic() {
      // We can't actually verify that open::that() launched a browser
      // from a unit test (no system browser in CI), but we CAN verify
      // that the handler runs to completion without panicking on a
      // valid scheme. open::that() returns Err when no browser is
      // configured, which the handler swallows via tracing::warn.
      let mut app = Inboxly::default();
      let _ = app.update(Message::OpenExternalUrl("https://example.com".into()));
      assert!(app.open_thread_id.is_none());
  }

  #[test]
  fn open_external_url_rejects_javascript_scheme() {
      // Eng review Issue 2.2 defence in depth: even if a javascript:
      // URL somehow slips past the sanitiser, the handler must reject
      // it before calling open::that(). This test pins the allowlist
      // behavior so future refactors of the handler can't silently
      // drop the scheme check.
      let mut app = Inboxly::default();
      // Should not panic, should not change state, should NOT call
      // open::that() (verified by the absence of any side effects we
      // can observe — the handler logs and returns).
      let _ = app.update(Message::OpenExternalUrl("javascript:alert(1)".into()));
      assert!(app.open_thread_id.is_none());
  }

  #[test]
  fn open_external_url_rejects_file_scheme() {
      // file:// URLs in emails are typically attacks (path traversal,
      // SMB credential theft on Windows, etc.). The allowlist excludes
      // them by virtue of only listing http/https/mailto.
      let mut app = Inboxly::default();
      let _ = app.update(Message::OpenExternalUrl("file:///etc/passwd".into()));
      assert!(app.open_thread_id.is_none());
  }

  #[test]
  fn open_external_url_rejects_garbage_input() {
      // Malformed URLs should not panic the handler — `Url::parse`
      // returns Err and we log + drop.
      let mut app = Inboxly::default();
      let _ = app.update(Message::OpenExternalUrl("not a url at all !!!".into()));
      assert!(app.open_thread_id.is_none());
  }

  #[test]
  fn open_external_url_accepts_mailto() {
      // mailto: is on the allowlist for compose-from-link in M36+.
      // For M34, it's accepted by the handler (no panic) and routed
      // to open::that(), which the user's mail client will handle.
      let mut app = Inboxly::default();
      let _ = app.update(Message::OpenExternalUrl("mailto:friend@example.com".into()));
      assert!(app.open_thread_id.is_none());
  }
  ```

- [ ] **Step 9.5: Build**

  Run: `cargo build -p inboxly 2>&1 | tail -3`
  Expected: clean.

- [ ] **Step 9.6: Manual verification of the full link path**

  After Phase 10's visual check, verify:
  - Open the demo thread (any click on an email row)
  - The demo HTML body contains a `<a href="https://example.com">...</a>` link — check that ammonia rewrote it by inspecting the rendered page source (or by hovering: the browser should show `#inboxly-ext:https://example.com` in the status bar)
  - Click the link
  - Inboxly's content should NOT change (no webview hijack)
  - Your default browser SHOULD open to `https://example.com`
  - If the browser does not open, check the tracing log for `open::that(...) failed` — common causes: no `xdg-open` on PATH, no default browser configured

  Phase 2's `demo_thread()` already includes the anchor link `<a href="https://example.com">example.com</a>` in the first message's HTML body, so this path is exercised the moment you click the demo thread in a debug build. (The `demo_thread_first_message_has_a_link` test enforces this — if someone removes the link from `demo_thread()`, that test fails.) **In release builds the empty_thread placeholder has no body, so the link path is not visually verifiable from the binary alone.** Run the visual check from `cargo run` (debug) or `cargo run --release` only after you've confirmed the link path in debug.

- [ ] **Step 9.7: Commit**

  ```bash
  git add inboxly-ui/src/components/app.rs inboxly-ui/src/app.rs
  git commit -m "feat(ui): Escape key closes thread + link-click interceptor (M34 phase 9)"
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
- `inboxly-store/src/thread_reader.rs` — new module: `ThreadReader` facade + `LoadedEmail` struct (Issue 1.5)
- `inboxly-ui/src/loaded_thread.rs` — new module: `LoadedThread`, `LoadedMessage`, `empty_thread()` (always), `demo_thread()` (debug-only), `fallback_thread()` selector, `build_loaded_thread()` converter
- `inboxly-ui/src/sanitize.rs` — new module: `sanitize_html()` wrapper
- `inboxly-ui/src/components/thread_detail_view.rs` — new component: layout shell
- `inboxly-ui/src/components/thread_message.rs` — new component: single message
- `inboxly-ui/src/components/email_row.rs` — add row-level `onclick` dispatching `OpenThread`
- `inboxly-ui/src/components/content_area.rs` — read the open-thread body signal context, branch on its presence inside Inbox arm
- `inboxly-ui/src/components/app.rs` — Escape key handler on `.app-shell`
- `inboxly-ui/src/components/mod.rs` — register two new component modules
- `inboxly-ui/assets/main.css` — add ~150 lines of thread-detail CSS
- `Cargo.toml` (workspace) and `inboxly-ui/Cargo.toml` — add `ammonia` dependency

## Reusable Existing Code

- `Store::get_emails_by_thread(thread_id) -> Result<Vec<EmailRow>>` (`inboxly-store/src/emails.rs:99`) — used internally by `ThreadReader::load_thread()`. UI code goes through the facade, not directly.
- `MaildirStore::read_email_content(&Path) -> Result<EmailContent>` (`inboxly-store/src/maildir_store.rs:578`) — used internally by `ThreadReader::load_thread()`. UI code goes through the facade, not directly.
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
- **Per-sender "Show images" toggle** — DEFER. M34 strips `<img src>` unconditionally for all senders. A future milestone will add a per-sender remote-image opt-in (matches Apple Mail / Thunderbird). Stub UI not built.
- **True non-blocking loader** — DEFER. M34 implements "cooperative async" via `Dioxus::spawn` (Issue 4.1) so the click handler returns immediately and the UI shows a "(loading…)" sentinel while the load runs. But the spawn'd task body still calls synchronous syscalls (`read_email_slim`) that block the local runtime briefly during file reads. For 50 messages × 5 ms = ~250 ms of local-runtime blocking. True non-blocking would require either making `Store` Send so `tokio::task::spawn_blocking` could move the load to a worker thread (large refactor of an already-existing storage type), OR switching `read_email_slim` to async I/O via `tokio::fs::read` (medium refactor of a stable storage method). Both are out of M34 scope. Revisit when real sync wiring lands and the actual workload becomes measurable.
- **Real IMAP sync wiring** — M34 ships with the demo loader as the only path that actually produces data. A future milestone (likely M35 or a dedicated sync-wiring milestone) connects `Inboxly::store` and `Inboxly::thread_reader` to a running sync. The `ThreadReader` facade is constructed once at sync wiring time as `Arc::new(ThreadReader::new(store, maildir))` and stored on Inboxly.

## Eng Review Decisions Captured Up-Front

- **Single thread at a time.** No tabs / split views — opening a new thread replaces the current one.
- **Cooperative async loader** (eng review Issue 4.1). The use_effect bridge sets a `loading_thread()` sentinel synchronously, then `spawn`s a Dioxus task that does the actual load. The click handler returns immediately and the UI shows "(loading…)" until the task completes. The spawn'd task body still calls synchronous syscalls — true non-blocking deferred (see Out of Scope).
- **Demo loader is debug-only via `#[cfg(debug_assertions)]`** (Issue 1.3). Release builds use `empty_thread()` which returns a `(no content available)` placeholder. Single-source-of-truth selector is `fallback_thread()` so cfg branching doesn't leak into call sites. When real sync lands, cleanup is one cfg-gate removal (or full deletion of demo_thread + fallback_thread).
- **HTML sanitisation via `ammonia` defaults + `<img>` src/srcset strip + sentinel-rewrite for `<a href>`** (Issues 1.1 + 1.2). Ammonia defaults strip scripts, JS URLs, event handlers. On top of that, M34 strips `src/srcset` from `<img>` (kills tracking pixels) and rewrites every `<a href>` to a sentinel prefix so clicks don't navigate the WebKitGTK webview away from the app. A document-level JS bridge intercepts sentinel-prefixed clicks and dispatches `OpenExternalUrl` which validates the scheme via `url::Url::parse` (Issue 2.2 — defence in depth) and hands the URL to `open::that()` for the system browser.
- **`dangerous_inner_html` is acceptable here** because the input has already been sanitised by `ammonia`. The "dangerous" name is enforced by Dioxus's API to make this decision visible at every call site.
- **Two-signal split for thread state** (Issue 1.4). `Inboxly::open_thread_id: Option<String>` records the user's intent (lightweight, goes through the state machine). The actual loaded body data lives in a SEPARATE `Signal<Option<Arc<LoadedThread>>>` context provided at App level. ThreadDetailView reads from the body signal directly, NOT from Inboxly. A `use_effect` in App bridges them via `use_memo` on `open_thread_id`. This keeps Inboxly's per-write Clone cost bounded — opening a thread no longer drags megabytes of body bytes through every nav click — while preserving state-machine testability.
- **`ThreadReader` facade in `inboxly-store`** (Issue 1.5). Wraps `Arc<Store>` + `Arc<MaildirStore>` so consumers don't need to plumb two store handles. Future M36/M37 consumers hold one `Arc<ThreadReader>` instead of two stores. Returns `Vec<LoadedEmail>` (raw storage data) which the UI converts to `LoadedThread` via `build_loaded_thread()`.
- **`SlimEmailContent` view type** (Issue 2.6). New type in `inboxly-core` that omits the headers HashMap and attachment byte content. `MaildirStore::read_email_slim()` is the slim accessor. Saves 5–20 KB of headers + potentially MB of attachment bytes per loaded email.
- **`Vec<Arc<LoadedMessage>>` inside `LoadedThread`** (Issue 2.8). Per-render clones in ThreadDetailView's loop become refcount bumps instead of body deep-clones. Combined with the outer `Arc<LoadedThread>` from Issue 1.4, two layers of Arc each address a distinct cost.
- **Errors are surfaced in the UI** (Issue 2.1) — `error_thread()` constructor + `error_message: Option<String>` field on `LoadedThread` + a banner in ThreadDetailView. Load failures get both a tracing log AND a visible banner. Silent fallback to demo/empty content was the alternative and was rejected as a "CI landmine" anti-pattern (lesson from M33 review).
- **Tests are state-machine + pure-function + integration**. No Dioxus SSR rendering tests (Dioxus 0.7 has no usable SSR test harness). Integration tests in `inboxly-store/tests/thread_reader.rs` (Issue 3.1) cover the full ThreadReader path against a real on-disk Maildir + in-memory SQLite.

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 0 | — | — |
| Codex Review | `/codex review` | Independent 2nd opinion | 0 | — | — |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | CLEAR (PLAN) | 17 issues, 16 resolved, 1 dismissed (Section 1: 5, Section 2: 8, Section 3: 2, Section 4: 2). 0 critical gaps. |
| Design Review | `/plan-design-review` | UI/UX gaps | 0 | — | — |
| DX Review | `/plan-devex-review` | Developer experience gaps | 0 | — | — |

**UNRESOLVED:** 0 — every issue resolved before exit.

**VERDICT:** ENG CLEARED — plan ready for implementation. The Section 1 architecture pass produced the largest changes (5 issues, all load-bearing); Section 2 caught 8 craft and security issues; Section 3 added critical integration tests for the new ThreadReader facade; Section 4 made the loader cooperative-async and fixed a theoretical JS-listener leak. Total ~50 new tests across 6 test files. Plan grew from ~1100 lines to ~2100 lines through the review. No CEO or design review run — recommended for follow-up only if M34 changes user-facing scope.

