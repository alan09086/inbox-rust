//! Root application component -- shell layout with theme context.

use std::sync::Arc;

use dioxus::prelude::*;
use inboxly_core::AuthMethod;

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

/// Run the SMTP send pipeline for the currently-Sending compose draft.
///
/// Hosted as a free function (gated behind `cfg(not(test))`) so the
/// Phase 12 send bridge in `App` can call into it without dragging a
/// pile of `#[cfg(test)]` arms through the closure body. The bridge is
/// a complete no-op in test builds — see the test arm of the
/// `use_effect` for `send_state_signal` below for the rationale.
///
/// This function:
///
/// 1. Snapshots the FROM `AccountConfig`, builds a `DraftEmail` via
///    [`compose_state_to_draft_email`], and captures the current
///    `save_generation` (Gemini G9 stale-result guard for failures
///    only — see step 4).
/// 2. Constructs an [`inboxly_imap::SmtpSender`]:
///      - `Password` / `AppPassword` -> reads the credential from the
///        `INBOXLY_SMTP_PASSWORD` env var. **M35b limitation**: there
///        is no keyring or per-account secrets store yet. M36 will
///        plumb a real secrets backend; until then the user must
///        `INBOXLY_SMTP_PASSWORD=foo cargo run` (or set it in the
///        desktop entry's `Exec=`) for manual end-to-end verification.
///      - `OAuth2` -> reports a "not yet wired" error. The
///        `SharedOAuth2` token cache lives in the IMAP sync loop and
///        is not threaded down to this bridge yet; plumbing it
///        through is M36 scope.
/// 3. Enters a retry loop using [`inboxly_imap::smtp::should_retry`]:
///    up to three attempts, with 1 s and 2 s delays for transient
///    errors, immediate stop for permanent rejections.
/// 4. **Stale-result guard for FAILURES ONLY** (Gemini G9 fix): on
///    send SUCCESS we ALWAYS commit the result -- the email already
///    left the wire and the user MUST be told, regardless of whether
///    they typed in the compose view during the send window. On send
///    FAILURE we drop the result if `save_generation` advanced, so a
///    stale failure can never clobber a fresh edit.
/// 5. On success the function:
///      - Enqueues an [`inboxly_core::OfflineAction::AppendSent`] so
///        the next replay pass copies the message to the IMAP Sent
///        folder (Gemini G6 fallback). The actual `APPEND` is deferred
///        because this bridge does not own a session.
///      - Deletes the SQLite draft row via `Store::delete_draft`.
///      - Calls [`inboxly_store::cleanup_draft_dir`] to remove the
///        per-draft attachment directory.
///      - Dispatches `Message::ComposeSendComplete { success: true }`
///        which transitions `send_state` to
///        `Sent { dismiss_pending: true }`. The `ComposeView` shows a
///        "Sent -- Dismiss" overlay until the user clicks dismiss.
/// 6. On failure the function dispatches
///    `Message::ComposeSendComplete { success: false, error: Some(redacted) }`
///    using [`inboxly_imap::smtp::redact_for_log`] so credentials and
///    PII never reach the UI's error banner.
#[cfg(not(test))]
async fn run_send_pipeline(mut app_state: Signal<Inboxly>) {
    // -- Snapshot pass: resolve the account, draft, and captured
    //    generation from a single `peek()`. peek does NOT subscribe
    //    so this task cannot re-fire its parent effect from inside.
    //    The borrow is dropped at the end of the inner block before
    //    any `app_state.write()` calls so the dispatch path is free
    //    of guard-overlap borrow issues (mirrors the Phase 10 bridge
    //    pattern).
    let (account_config_opt, draft_opt, captured_generation) = {
        let snapshot = app_state.peek();
        let account_config = snapshot
            .accounts
            .get(snapshot.compose.from_account_index)
            .cloned();
        let draft = match account_config.as_ref() {
            Some(cfg) => {
                let account_id = account_id_from_email(&cfg.email);
                compose_state_to_draft_email(&snapshot.compose, account_id)
            }
            None => None,
        };
        let captured_generation = snapshot.compose.save_generation;
        (account_config, draft, captured_generation)
    };

    let Some(account_config) = account_config_opt else {
        app_state.write().update(Message::ComposeSendComplete {
            success: false,
            error: Some("no account configured for this draft".to_string()),
        });
        return;
    };
    let Some(draft) = draft_opt else {
        app_state.write().update(Message::ComposeSendComplete {
            success: false,
            error: Some("compose has no draft_id (OpenCompose did not run)".to_string()),
        });
        return;
    };

    // -- Build the SmtpSender for the resolved auth method.
    let sender = match account_config.auth_method {
        AuthMethod::Password | AuthMethod::AppPassword => {
            let Some(password) = std::env::var("INBOXLY_SMTP_PASSWORD").ok() else {
                app_state.write().update(Message::ComposeSendComplete {
                    success: false,
                    error: Some(
                        "password required but INBOXLY_SMTP_PASSWORD env var is not set (M35b limitation; M36 will add a keyring)"
                            .to_string(),
                    ),
                });
                return;
            };
            inboxly_imap::SmtpSender::with_password(account_config.clone(), password)
        }
        AuthMethod::OAuth2 => {
            app_state.write().update(Message::ComposeSendComplete {
                success: false,
                error: Some(
                    "OAuth2 SMTP send is not yet wired through the compose bridge -- M36 will plumb SharedOAuth2 from the sync loop"
                        .to_string(),
                ),
            });
            return;
        }
    };

    // -- Retry loop. Up to three attempts: 1 s delay, 2 s delay,
    //    then stop. Permanent errors (5xx, malformed message) stop
    //    immediately regardless of attempt number.
    let send_result: Result<(), inboxly_imap::smtp::SmtpError> = {
        use inboxly_imap::smtp::{RetryDecision, should_retry};

        let mut attempt: u32 = 0;
        loop {
            attempt = attempt.saturating_add(1);
            match sender.send(&draft).await {
                Ok(()) => break Ok(()),
                Err(err) => match should_retry(&err, attempt) {
                    RetryDecision::Retry { delay_ms } => {
                        tracing::warn!(
                            attempt,
                            delay_ms,
                            error = ?err,
                            "SMTP send failed, will retry"
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    }
                    RetryDecision::Stop => break Err(err),
                },
            }
        }
    };

    match send_result {
        Ok(()) => {
            tracing::info!(message_id = %draft.message_id, "SMTP send succeeded");

            // -- Step 5a: enqueue the AppendSent fallback so the next
            //    replay pass writes the IMAP Sent folder copy. The
            //    bridge does not own an IMAP session so it cannot
            //    APPEND directly; the queued action is the recovery
            //    path.
            let store_handle = app_state.peek().store.as_ref().cloned();
            if let Some(store) = store_handle.as_ref() {
                let queue_action = inboxly_core::OfflineAction::AppendSent {
                    account_id: account_config.email.clone(),
                    draft_message_id: draft.message_id.clone(),
                };
                match serde_json::to_string(&queue_action) {
                    Ok(payload) => {
                        if let Err(e) =
                            store.enqueue_offline_action(queue_action.variant_name(), &payload)
                        {
                            tracing::warn!(
                                error = %e,
                                "failed to enqueue AppendSent offline action"
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "failed to serialize AppendSent offline action"
                        );
                    }
                }
            }

            // -- Step 5b: delete the SQLite draft row + clean up the
            //    per-draft attachment directory. The
            //    DismissSentNotice handler will call
            //    cleanup_draft_dir again as a safety net -- that
            //    second call is a no-op when the dir is already
            //    gone.
            if let Some(store) = store_handle
                && let Err(e) = store.delete_draft(&draft.id)
            {
                tracing::warn!(
                    draft_id = %draft.id,
                    error = %e,
                    "failed to delete draft after send"
                );
            }
            if let Err(e) = inboxly_store::cleanup_draft_dir(&draft.id) {
                tracing::warn!(
                    draft_id = %draft.id,
                    error = %e,
                    "failed to cleanup draft attachment dir after send"
                );
            }

            // captured_generation is intentionally unused on the
            // success path -- Gemini G9 mandates we ALWAYS commit
            // success regardless of generation drift.
            let _ = captured_generation;

            app_state.write().update(Message::ComposeSendComplete {
                success: true,
                error: None,
            });
        }
        Err(err) => {
            // Stale-result guard FOR FAILURES ONLY (Gemini G9 fix):
            // if the user typed during the send window, drop the
            // failure rather than clobber the fresh edit.
            let current_generation = app_state.peek().compose.save_generation;
            if current_generation != captured_generation {
                tracing::warn!(
                    captured = captured_generation,
                    current = current_generation,
                    "SMTP send failed but compose generation drifted -- dropping stale failure"
                );
                return;
            }
            let redacted = inboxly_imap::smtp::redact_for_log(&err, &draft);
            tracing::warn!(error = %redacted, "SMTP send failed");
            app_state.write().update(Message::ComposeSendComplete {
                success: false,
                error: Some(redacted),
            });
        }
    }
}

/// Main stylesheet source embedded at compile time.
///
/// Dioxus 0.7's `document::Stylesheet { href: CSS }` pattern with `asset!()`
/// is supposed to serve the asset path through the desktop webview's custom
/// protocol, but for this build the CSS doesn't get fetched at runtime —
/// everything that relies on CSS classes falls back to browser defaults.
/// Embedding via `include_str!` and injecting through a `<style>` tag with
/// `dangerous_inner_html` in the App component guarantees the styles reach
/// the DOM. See `components/app.rs::App` for the inline injection.
static CSS_INLINE: &str = include_str!("../../assets/main.css");

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

    // ===== M35 Phase 12: Compose send bridge =====
    //
    // Watches `compose.send_state`. When the
    // `Message::ComposeSendDraft` handler transitions
    // `Idle -> Sending`, this effect spawns
    // [`run_send_pipeline`] (defined above) which owns the entire
    // SMTP -> retry -> AppendSent -> dismiss flow. The pipeline is a
    // hard no-op in test builds (M34 side-effects-in-tests
    // precedent). See `run_send_pipeline`'s doc comment for the full
    // success/failure/Gemini G6/G9 semantics.
    let send_state_signal = use_memo(move || {
        let s = app_state.read();
        matches!(s.compose.send_state, ComposeSendState::Sending)
    });
    use_effect(move || {
        let is_sending = *send_state_signal.read();
        if !is_sending {
            return;
        }
        let app_state = app_state;
        spawn(async move {
            // The entire send pipeline is a no-op in test builds. The
            // bridge would otherwise need to dial real SMTP servers,
            // touch the user's SQLite store, and read environment
            // variables -- side effects that are forbidden in tests
            // per the M34 incident precedent. Tests that exercise
            // the state machine after a "send" should dispatch
            // `Message::ComposeSendComplete` directly rather than
            // relying on this bridge.
            #[cfg(not(test))]
            run_send_pipeline(app_state).await;
            #[cfg(test)]
            {
                let _ = app_state;
            }
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
        // Dioxus 0.7's `document::Stylesheet { href: CSS }` pattern does not
        // reach the desktop webview in our current setup (the asset protocol
        // returns nothing and every .css-class rule falls back to browser
        // defaults). The inline `<style>` tag below embeds the same CSS via
        // `include_str!` and guarantees the rules land in the DOM.
        style { dangerous_inner_html: CSS_INLINE }

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

        // SpeedDialFab dispatches OpenCompose on click. Hide it in Compose
        // view so (a) it doesn't visually stack on top of the Send button
        // in the compose footer and (b) clicking it mid-compose doesn't
        // reset the current draft via the "idempotent second OpenCompose"
        // branch. Also hide in Settings per the existing rule.
        if !matches!(active_view, ActiveView::Settings | ActiveView::Compose) {
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
