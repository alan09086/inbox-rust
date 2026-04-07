//! Root application component -- shell layout with theme context.

use std::sync::Arc;

use dioxus::prelude::*;

use crate::app::{Inboxly, Message};
use crate::components::content_area::ContentArea;
use crate::components::nav_drawer::NavDrawer;
use crate::components::snooze_picker::SnoozePicker;
use crate::components::speed_dial_fab::SpeedDialFab;
use crate::components::toolbar::Toolbar;
use crate::components::undo_snackbar::UndoSnackbar;
use crate::loaded_thread::{
    LoadedThread, build_loaded_thread, error_thread, fallback_thread, loading_thread,
};
use crate::state::{ComposeSendState, ComposeState};
use crate::theme::{ActiveView, ThemeConfig};

/// UUID v5 namespace for deriving deterministic [`inboxly_core::AccountId`]
/// values from an account's email address.
///
/// `AccountConfig` carries an `email` field but no UUID id (see
/// `inboxly-core/src/config.rs`). The drafts table column, on the other
/// hand, expects an `inboxly_core::AccountId` (a `Uuid`). Until the
/// account model is refactored to carry a real id (out of M35b scope),
/// the auto-save bridge derives one deterministically from the email so
/// that successive saves of the same draft target the same row, and so
/// the Phase 12 send pipeline can resolve the same id from the same
/// account without coordination.
///
/// The namespace UUID below is a fixed v4 token chosen specifically for
/// inboxly compose drafts — it has no special meaning beyond being a
/// stable seed. Do NOT change it without a migration plan: every
/// existing draft row references AccountIds derived from this namespace.
const COMPOSE_ACCOUNT_NAMESPACE: uuid::Uuid = uuid::uuid!("a4b3c4d5-e6f7-4a8b-9c0d-1e2f3a4b5c6d");

/// Derive a deterministic [`inboxly_core::AccountId`] from an email
/// address. See [`COMPOSE_ACCOUNT_NAMESPACE`] for the rationale.
fn account_id_from_email(email: &str) -> inboxly_core::AccountId {
    inboxly_core::AccountId(uuid::Uuid::new_v5(
        &COMPOSE_ACCOUNT_NAMESPACE,
        email.as_bytes(),
    ))
}

/// Build a [`inboxly_core::DraftEmail`] snapshot from the current
/// [`ComposeState`].
///
/// The compose state holds `Arc`-wrapped contacts and attachments for
/// cheap per-render clones; this helper materialises them into the owned
/// shape the storage layer expects. Returns `None` if `compose.draft_id`
/// is unset (which shouldn't happen after `OpenCompose` — Gemini G4
/// ensures eager id assignment — but defensively skip rather than panic).
///
/// Used by:
/// - The Phase 10 auto-save bridge (30 s timer)
/// - (Future) the Phase 12 send bridge to build the SMTP sender input
pub(crate) fn compose_state_to_draft_email(
    compose: &ComposeState,
    account_id: inboxly_core::AccountId,
) -> Option<inboxly_core::DraftEmail> {
    use chrono::Utc;
    use inboxly_core::DraftEmail;

    let draft_id = compose.draft_id.clone()?;
    let message_id = format!("<{draft_id}@inboxly.local>");
    let now = Utc::now();

    Some(DraftEmail {
        id: draft_id,
        account_id,
        message_id,
        subject: compose.subject.clone(),
        body_markdown: compose.body_markdown.clone(),
        to: compose.to.iter().map(|arc| (**arc).clone()).collect(),
        cc: compose.cc.iter().map(|arc| (**arc).clone()).collect(),
        bcc: compose.bcc.iter().map(|arc| (**arc).clone()).collect(),
        attachments: compose
            .attachments
            .iter()
            .map(|arc| (**arc).clone())
            .collect(),
        mode: compose.mode.clone(),
        in_reply_to: None, // M36
        references: None,  // M36
        maildir_path: None,
        // Auto-save doesn't track creation separately from updates; both
        // timestamps reflect the same instant. The storage layer's
        // `update_draft` deliberately ignores `created_at` (see
        // `inboxly-store/src/drafts.rs`), so the only consequence here is
        // that the very first `insert_draft` call records "created at the
        // first save" rather than "created at compose-open".
        created_at: now,
        updated_at: now,
    })
}

/// Infer a MIME type from a filename's extension.
///
/// Used by the Phase 11 attachment picker bridge to populate
/// `AttachmentDraft::mime_type` from the user-picked filename. Covers
/// the common attachment types (PDFs, images, archives, Office docs,
/// plain text variants, JSON/XML); everything else falls through to
/// `application/octet-stream`.
///
/// The match is case-insensitive on the extension. Returns owned
/// `String` because the call site stores it in [`inboxly_core::AttachmentDraft`].
///
/// This deliberately does NOT use the `mime_guess` crate — the workspace
/// already keeps the dependency surface tight, and the ~20 entries below
/// cover the realistic compose-attachment universe. If a use case
/// emerges that needs broader coverage, swap to `mime_guess` then.
fn mime_from_extension(filename: &str) -> String {
    let ext = std::path::Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .unwrap_or_default();
    match ext.as_str() {
        "pdf" => "application/pdf",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "txt" | "md" => "text/plain",
        "html" | "htm" => "text/html",
        "csv" => "text/csv",
        "json" => "application/json",
        "xml" => "application/xml",
        "zip" => "application/zip",
        "tar" => "application/x-tar",
        "gz" => "application/gzip",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        _ => "application/octet-stream",
    }
    .to_string()
}

/// Window title -- updates reactively based on active view.
#[allow(dead_code)]
fn window_title(view: ActiveView) -> String {
    format!("Inboxly \u{2014} {}", view.title())
}

/// CSS asset for the main stylesheet.
static CSS: Asset = asset!("/assets/main.css");

/// Root application component.
///
/// Creates the `Inboxly` state and provides it as context to all children.
/// Sets the `data-theme` attribute for CSS theming, and renders the shell
/// layout: toolbar + body row (nav drawer + content area).
#[component]
pub fn App() -> Element {
    // Initialise app state once on first render.
    let app_state = use_context_provider(|| {
        Signal::new(Inboxly {
            theme: ThemeConfig::from_system(),
            ..Inboxly::default()
        })
    });

    // Body data context — separate from Inboxly so per-write clones
    // don't drag the thread body bytes around. ThreadDetailView reads
    // from this signal directly. (Eng review Issue 1.4.)
    let mut open_thread = use_context_provider(|| Signal::new(None::<Arc<LoadedThread>>));

    // Bridge: watch Inboxly::open_thread_id (the intent), and when it
    // changes, run the loader through the ThreadReader facade and
    // write the result into open_thread (the body). use_memo on the
    // id ensures the effect only re-runs when intent changes, not on
    // every Inboxly write.
    let open_id = use_memo(move || app_state.read().open_thread_id.clone());
    use_effect(move || {
        let id_opt = open_id.read().clone();
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
                                        tracing::warn!("build_loaded_thread({id}) failed: {e}");
                                        error_thread(
                                            &id,
                                            format!("Failed to build thread view: {e}"),
                                        )
                                    }
                                },
                                Err(e) => {
                                    tracing::warn!("ThreadReader::load_thread({id}) failed: {e}");
                                    error_thread(&id, format!("Failed to load thread: {e}"))
                                }
                            }
                        }
                        None => fallback_thread(&id), // no reader wired — not an error
                    };
                    drop(snapshot);

                    // F-1: stale-result guard. If the user has navigated away
                    // (or to a different thread) while we were loading, drop
                    // the result instead of clobbering the current state.
                    // Without this, a slow load for thread A that finishes
                    // after the user clicks thread B would overwrite B's
                    // content with A's. M34's in-memory fallback_thread() is
                    // always fast so the race is latent, but once M36/M37
                    // wires the real ThreadReader on cold file reads this
                    // becomes a user-visible bug.
                    let still_current = app_state.peek().open_thread_id.as_deref() == Some(&id);
                    if !still_current {
                        tracing::debug!(
                            "ThreadReader: dropping stale load result for thread {id} (user navigated away)"
                        );
                        return;
                    }

                    open_thread.set(Some(Arc::new(loaded)));
                });
            }
            None => {
                open_thread.set(None);
            }
        }
    });

    // ===== M35 Phase 10: Compose auto-save bridge =====
    //
    // Watches the compose dirty flag + save_generation. When the user has
    // unsaved changes, spawn a 30-second timer task. On tick, verify the
    // state machine is still in a savable state (NOT Sending/Sent —
    // Gemini G5 handoff to the send pipeline) and the user hasn't already
    // saved (dirty still true). Capture the save_generation BEFORE the
    // disk write (Issue 1.8) so the post-save dispatch can decide whether
    // the user typed mid-save and the dirty flag should stay set.
    //
    // The Store::insert_draft / Store::update_draft calls are gated
    // behind cfg(not(test)) per the M34 side-effects-in-tests precedent
    // (overnight test runs must not mutate the user's SQLite store).
    //
    // The reactive memo deliberately combines (dirty, save_generation,
    // has_id) so a single rapid edit (which bumps save_generation but
    // leaves dirty=true) re-fires the effect — that's the desired
    // behaviour: each user edit resets the 30 s clock by spawning a
    // fresh timer; the captured generation guard prevents the older
    // timer from clearing dirty after the newer save has already run.
    let compose_dirty_signal = use_memo(move || {
        let s = app_state.read();
        (
            s.compose.dirty,
            s.compose.save_generation,
            s.compose.draft_id.is_some(),
        )
    });
    use_effect(move || {
        let (dirty, _generation, has_id) = *compose_dirty_signal.read();
        if !dirty || !has_id {
            return;
        }
        let mut app_state = app_state;
        spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;

            // Resolve everything we need from a single peek() — peek
            // does NOT subscribe, so re-firing this effect is impossible
            // from within the spawned task. We extract owned values
            // before the .await-free section ends so the borrow guard
            // is dropped before any further work.
            let (send_state_is_send_pipeline_owned, still_dirty, captured_generation, draft, store) = {
                let snapshot = app_state.peek();

                // Gemini G5: abort if we entered Sending/Sent state
                // during the sleep — the send pipeline owns canonical
                // state during a send.
                let send_state_is_send_pipeline_owned = matches!(
                    snapshot.compose.send_state,
                    ComposeSendState::Sending | ComposeSendState::Sent { .. }
                );

                // Re-check dirty: someone may have already saved.
                let still_dirty = snapshot.compose.dirty;

                // Issue 1.8: capture the generation BEFORE writing,
                // not after. The dispatch back to the handler uses
                // this value as the "if generation still matches,
                // clear dirty" guard.
                let captured_generation = snapshot.compose.save_generation;

                // Resolve the FROM account email so we can derive the
                // deterministic AccountId. Phase 12 will resolve the
                // same id from the same account, so the auto-save row
                // and the send pipeline target the same draft.
                let account_email = snapshot
                    .accounts
                    .get(snapshot.compose.from_account_index)
                    .map(|a| a.email.clone());

                let draft = match account_email {
                    Some(email) => {
                        let account_id = account_id_from_email(&email);
                        compose_state_to_draft_email(&snapshot.compose, account_id)
                    }
                    None => None,
                };

                let store = snapshot.store.clone();

                (
                    send_state_is_send_pipeline_owned,
                    still_dirty,
                    captured_generation,
                    draft,
                    store,
                )
            };

            if send_state_is_send_pipeline_owned {
                tracing::debug!(
                    "compose auto-save aborted: send_state is Sending/Sent (Gemini G5)"
                );
                return;
            }
            if !still_dirty {
                // Someone already saved (manual save in a future phase,
                // or this is a stale timer from a now-cleared dirty
                // flag). No-op.
                return;
            }
            let Some(draft) = draft else {
                tracing::warn!("compose auto-save: no FROM account or no draft_id, skipping save");
                // Still dispatch the tick so the handler can clear
                // dirty if generation matches — there's nothing to
                // persist but we don't want the bridge to keep firing.
                app_state.write().update(Message::ComposeAutoSaveTick {
                    generation: captured_generation,
                });
                return;
            };

            // Gated behind cfg(not(test)) per the M34 side-effects-in-
            // tests precedent. Overnight test runs must never write to
            // the user's real SQLite store.
            #[cfg(not(test))]
            if let Some(store) = store.as_ref() {
                // Try update first (the row should exist on every
                // save after the first); fall back to insert on
                // NotFound for the very first save of a brand-new
                // draft. Other errors are logged and the tick is
                // still dispatched so the bridge doesn't retry on
                // every keystroke.
                match store.update_draft(&draft) {
                    Ok(()) => {
                        tracing::debug!(
                            draft_id = %draft.id,
                            "compose auto-save: update_draft ok"
                        );
                    }
                    Err(inboxly_store::StoreError::NotFound(_)) => {
                        match store.insert_draft(&draft) {
                            Ok(()) => {
                                tracing::debug!(
                                    draft_id = %draft.id,
                                    "compose auto-save: insert_draft ok"
                                );
                            }
                            Err(e) => {
                                tracing::warn!(
                                    draft_id = %draft.id,
                                    error = %e,
                                    "compose auto-save: insert_draft failed"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            draft_id = %draft.id,
                            error = %e,
                            "compose auto-save: update_draft failed"
                        );
                    }
                }
            }
            #[cfg(test)]
            {
                let _ = store; // silence unused warning under cfg(test)
                tracing::debug!(
                    draft_id = %draft.id,
                    "compose auto-save: store write skipped in test mode"
                );
            }

            // Dispatch the tick — the handler clears dirty IFF the
            // captured generation still matches (Issue 1.8 stale-
            // result guard). If the user typed during the save, the
            // generation will have advanced and dirty stays true so
            // the next tick re-saves.
            app_state.write().update(Message::ComposeAutoSaveTick {
                generation: captured_generation,
            });
        });
    });

    // ===== M35 Phase 11: Compose attachment picker bridge =====
    //
    // Watches `compose.attach_picker_counter`, which the
    // `Message::ComposeAttachFile` handler bumps. When the counter
    // changes (and is non-zero — initial mount renders with counter=0
    // and we deliberately do nothing), the bridge spawns a task that:
    //
    //   1. Snapshots `draft_id` and the current total attachment size
    //      from a single `peek()` (peek does NOT subscribe, so the
    //      spawned task can never re-fire the effect from inside).
    //   2. Opens `rfd::AsyncFileDialog::pick_file()` — gated behind
    //      cfg(not(test)) so test runs never open a real dialog
    //      (M34 side-effects-in-tests precedent).
    //   3. Reads file *metadata* via `tokio::fs::metadata` — NOT the
    //      bytes (Gemini G3 metadata-first). The 20 MB total cap is
    //      enforced from the metadata size before any copy work.
    //   4. Generates a UUID-suffixed disk filename via
    //      `inboxly_store::draft_attachments::make_draft_filename`
    //      (Gemini G2 collision-resistant naming) and copies the
    //      source into the per-draft directory. The copy is also
    //      gated behind cfg(not(test)).
    //   5. Builds an `AttachmentDraft` whose `filename` is the ORIGINAL
    //      name (for the chip and the eventual MIME `Content-
    //      Disposition` header) and whose `source` points at the
    //      UUID-suffixed disk path.
    //   6. Dispatches `Message::ComposeAttachmentAdded` so the pure
    //      state machine can register the new attachment.
    //
    // Errors from any step are logged via `tracing::warn!` and the
    // bridge silently aborts — Phase 12 / Phase 13 surface a snackbar.
    let attach_counter_signal = use_memo(move || {
        let s = app_state.read();
        s.compose.attach_picker_counter
    });
    use_effect(move || {
        let counter = *attach_counter_signal.read();
        if counter == 0 {
            // Initial render — Phase 6's `ComposeState::new` starts
            // the counter at 0 so the bridge does not pop a dialog
            // on app start.
            return;
        }
        let mut app_state = app_state;
        spawn(async move {
            // Snapshot the draft_id and current total attachment size
            // from a single peek() — peek does NOT subscribe, so this
            // spawned task can never re-fire the parent effect from
            // within. Mirrors the auto-save bridge pattern.
            let (draft_id, current_total_size) = {
                let snapshot = app_state.peek();
                let Some(draft_id) = snapshot.compose.draft_id.clone() else {
                    tracing::warn!(
                        "attach picker: no draft_id on ComposeState, aborting (OpenCompose should set this eagerly per Gemini G4)"
                    );
                    return;
                };
                let total: u64 = snapshot
                    .compose
                    .attachments
                    .iter()
                    .map(|a| a.size_bytes)
                    .sum();
                (draft_id, total)
            };

            // Step 2: open the file picker. Gated behind cfg(not(test))
            // — test runs must NEVER pop a dialog (M34 precedent).
            #[cfg(not(test))]
            let picked_path: Option<std::path::PathBuf> = {
                use rfd::AsyncFileDialog;
                AsyncFileDialog::new()
                    .add_filter("All files", &["*"])
                    .pick_file()
                    .await
                    .map(|h| h.path().to_path_buf())
            };
            #[cfg(test)]
            let picked_path: Option<std::path::PathBuf> = None;

            let Some(src_path) = picked_path else {
                // User cancelled the dialog (or test mode no-op).
                return;
            };

            let original_name = src_path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "attachment".to_string());

            // Step 3: metadata-first size check (Gemini G3) — never
            // load the file bytes into memory.
            let size = match tokio::fs::metadata(&src_path).await {
                Ok(m) => m.len(),
                Err(e) => {
                    tracing::warn!(
                        path = %src_path.display(),
                        error = %e,
                        "attach picker: failed to stat source file"
                    );
                    return;
                }
            };

            /// Hard 20 MB cap on the *total* draft attachment size,
            /// applied across the existing attachments plus the
            /// candidate. Matches the cap used by the
            /// `ComposeAttachmentTooLarge` snackbar.
            const MAX_TOTAL_BYTES: u64 = 20 * 1024 * 1024;
            // saturating_add: defensive against pathological inputs
            // (a 18 EB file would otherwise wrap, and clippy
            // arithmetic_side_effects would catch a bare `+`).
            if current_total_size.saturating_add(size) > MAX_TOTAL_BYTES {
                tracing::warn!(
                    filename = %original_name,
                    size_bytes = size,
                    current_total_bytes = current_total_size,
                    "attach picker: file would push compose draft over the 20 MB cap"
                );
                app_state.write().update(Message::ComposeAttachmentTooLarge);
                return;
            }

            // Step 4: per-draft directory + UUID-suffixed filename.
            let dir = match inboxly_store::draft_attachments::ensure_draft_dir(&draft_id) {
                Ok(d) => d,
                Err(e) => {
                    tracing::warn!(
                        draft_id = %draft_id,
                        error = %e,
                        "attach picker: ensure_draft_dir failed"
                    );
                    return;
                }
            };
            let uuid = uuid::Uuid::new_v4();
            let disk_name =
                inboxly_store::draft_attachments::make_draft_filename(&original_name, uuid);
            let dest = dir.join(&disk_name);

            // Copy the source into the per-draft directory. Gated
            // behind cfg(not(test)) so test runs never touch disk.
            #[cfg(not(test))]
            if let Err(e) = tokio::fs::copy(&src_path, &dest).await {
                tracing::warn!(
                    src = %src_path.display(),
                    dest = %dest.display(),
                    error = %e,
                    "attach picker: copy to per-draft directory failed"
                );
                return;
            }
            #[cfg(test)]
            {
                // Suppress unused-variable warnings under cfg(test).
                let _ = &src_path;
                let _ = &dest;
            }

            let mime_type = mime_from_extension(&original_name);

            let att = inboxly_core::AttachmentDraft {
                filename: original_name,
                mime_type,
                size_bytes: size,
                source: inboxly_core::AttachmentSource::Disk(dest),
            };

            app_state
                .write()
                .update(Message::ComposeAttachmentAdded(Arc::new(att)));
        });
    });

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
            let mut ch = document::eval(
                r#"
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
                "#,
            );
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
        let _ = document::eval(
            r#"
            if (window.__inboxly_escape_handler) {
                document.removeEventListener('keydown', window.__inboxly_escape_handler, true);
                delete window.__inboxly_escape_handler;
            }
            "#,
        );
    });

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
        let _ = document::eval(
            r#"
            if (window.__inboxly_link_click_handler) {
                document.removeEventListener('click', window.__inboxly_link_click_handler, true);
                delete window.__inboxly_link_click_handler;
            }
            "#,
        );
    });

    let is_dark = app_state.read().theme.colors.is_dark;
    let drawer_open = app_state.read().drawer_open;
    let active_view = app_state.read().active_view;

    rsx! {
        document::Stylesheet { href: CSS }

        div {
            class: "app-shell",
            "data-theme": if is_dark { "dark" } else { "light" },

            Toolbar {}

            div {
                class: "body-row",

                if drawer_open && active_view != ActiveView::Settings {
                    NavDrawer {}
                }

                ContentArea {}
            }
        }

        UndoSnackbar {}

        if active_view != ActiveView::Settings {
            SpeedDialFab {}
            SnoozePicker {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use inboxly_core::Contact;

    use super::{ComposeState, account_id_from_email, compose_state_to_draft_email};

    /// `compose_state_to_draft_email` returns `None` when the compose
    /// state has no `draft_id`. The bridge depends on this so it can
    /// safely skip the disk write rather than panicking on a half-
    /// initialised state.
    #[test]
    fn helper_returns_none_without_draft_id() {
        let compose = ComposeState::new();
        assert!(compose.draft_id.is_none());
        let id = account_id_from_email("alice@example.com");
        assert!(compose_state_to_draft_email(&compose, id).is_none());
    }

    /// Happy path: with a `draft_id` set, the helper materialises the
    /// `Arc`-wrapped recipients into owned `Contact` values, copies the
    /// subject/body, and stamps a Message-ID derived from the draft id.
    #[test]
    fn helper_materialises_arc_recipients_into_owned() {
        let mut compose = ComposeState::new();
        compose.draft_id = Some("d8a17e3b-1234-4abc-9def-000000000001".to_string());
        compose.subject = "Hello".to_string();
        compose.body_markdown = "Body".to_string();
        compose
            .to
            .push(Arc::new(Contact::new("Bob", "bob@example.com")));

        let id = account_id_from_email("alice@example.com");
        let draft = compose_state_to_draft_email(&compose, id)
            .expect("draft_id is set, helper must return Some");
        assert_eq!(draft.id, "d8a17e3b-1234-4abc-9def-000000000001");
        assert_eq!(draft.subject, "Hello");
        assert_eq!(draft.body_markdown, "Body");
        assert_eq!(
            draft.message_id,
            "<d8a17e3b-1234-4abc-9def-000000000001@inboxly.local>"
        );
        assert_eq!(draft.to.len(), 1);
        assert_eq!(draft.to[0].address, "bob@example.com");
        assert_eq!(draft.account_id, id);
    }

    /// `account_id_from_email` is deterministic — same email always
    /// produces the same `AccountId`. The auto-save bridge relies on
    /// this so successive saves of the same draft target the same row,
    /// and Phase 12's send pipeline can resolve the same id.
    #[test]
    fn account_id_from_email_is_deterministic() {
        let a = account_id_from_email("alice@example.com");
        let b = account_id_from_email("alice@example.com");
        let c = account_id_from_email("bob@example.com");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
