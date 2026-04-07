//! Root application component -- shell layout with theme context.

use std::sync::Arc;

use dioxus::prelude::*;

use crate::app::Inboxly;
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
                    open_thread.set(Some(Arc::new(loaded)));
                });
            }
            None => {
                open_thread.set(None);
            }
        }
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
