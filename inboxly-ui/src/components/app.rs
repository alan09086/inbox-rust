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
    build_loaded_thread, error_thread, fallback_thread, loading_thread, LoadedThread,
};
use crate::theme::{ActiveView, ThemeConfig};

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
    let mut open_thread =
        use_context_provider(|| Signal::new(None::<Arc<LoadedThread>>));

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
                    let still_current = app_state
                        .peek()
                        .open_thread_id
                        .as_deref()
                        == Some(&id);
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
