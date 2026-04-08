//! In-progress compose draft state (M35 Phase 6).
//!
//! Owned by [`crate::app::Inboxly`] as `pub compose: ComposeState`. The
//! field lifecycle is driven by the `Compose*` variants of
//! [`crate::app::Message`]:
//!
//! - `OpenCompose` initializes a fresh `ComposeState` with a UUID `draft_id`
//!   (Gemini G4 — eager id so the per-draft attachment directory can exist
//!   before the first auto-save tick)
//! - field-change variants set `dirty = true` and bump `save_generation`
//!   via [`ComposeState::mark_dirty`]
//! - `CloseCompose` restores `active_view` but leaves the state intact
//! - `ComposeDiscardDraft` resets the state to `default()`
//! - `ComposeSendComplete { success: true }` transitions `send_state` to
//!   `Sent { dismiss_pending: true }` (Gemini G9 two-phase commit)
//! - `ComposeDismissSentNotice` clears `Sent` → `Idle` and resets the
//!   compose state

use std::sync::Arc;

use inboxly_core::{AttachmentDraft, ComposeMode, Contact};

/// State backing the compose view.
///
/// `Debug` + `Clone` are required because [`crate::app::Message`] derives
/// both, and the M36 Phase 8 [`crate::app::Message::ComposeReplyReady`]
/// variant carries `Box<ComposeState>` (boxed to keep the enum variant
/// size below clippy's `large_enum_variant` 200-byte threshold). Cloning
/// is cheap relative to the field count: every `Vec` field holds
/// `Arc`-wrapped contents (`Arc<Contact>`, `Arc<AttachmentDraft>`), so a
/// `ComposeState::clone` is mostly refcount bumps plus a handful of
/// `String::clone` calls.
#[derive(Debug, Clone)]
pub struct ComposeState {
    /// UUID of the in-progress draft. Set by `OpenCompose` (eager — Gemini
    /// G4) so the per-draft attachment directory can be created before the
    /// first auto-save tick. `None` when no compose has ever been opened.
    pub draft_id: Option<String>,

    // -- Header fields --
    /// Subject line of the draft.
    pub subject: String,
    /// Account index (into `Inboxly::accounts`) the user wants to send
    /// FROM (Issue 1.3 — account picker dropdown). Defaults to 0 (the
    /// active account at compose-open time).
    pub from_account_index: usize,

    // -- Recipients (Arc-wrapped per Issue 4.2 so per-render clones are
    //    refcount bumps rather than full Contact clones).
    /// Resolved To recipients.
    pub to: Vec<Arc<Contact>>,
    /// Resolved Cc recipients.
    pub cc: Vec<Arc<Contact>>,
    /// Resolved Bcc recipients.
    pub bcc: Vec<Arc<Contact>>,
    /// Current text in the To input field (chips are added on Enter/comma).
    pub to_input: String,
    /// Current text in the Cc input field.
    pub cc_input: String,
    /// Current text in the Bcc input field.
    pub bcc_input: String,
    /// Whether the Cc/Bcc rows are visible. Collapsed by default.
    pub show_cc_bcc: bool,

    // -- Body --
    /// The Markdown source the user is editing.
    pub body_markdown: String,
    /// Toggle between body textarea and rendered Markdown preview.
    pub show_preview: bool,

    // -- Attachments (Arc-wrapped per Issue 4.2).
    /// Files the user has attached so far.
    pub attachments: Vec<Arc<AttachmentDraft>>,

    // -- Mode (M35b is always New; Reply/ReplyAll/Forward are M36
    //    placeholders that the storage layer already understands).
    /// Compose mode (always `New` in M35b).
    pub mode: ComposeMode,

    // -- Reply threading headers (M36 Phase 7) --
    /// RFC 5322 `In-Reply-To` header value. Set by
    /// [`crate::components::app::compose_state_from_original`] for
    /// `Reply` / `ReplyAll` modes; left `None` for `New` and `Forward`
    /// (Forward starts a new thread per Gmail convention).
    pub in_reply_to: Option<String>,

    /// RFC 5322 `References` header value (JWZ threading chain).
    /// Set by [`crate::components::app::compose_state_from_original`]
    /// for `Reply` / `ReplyAll`; left `None` for `New` and `Forward`.
    /// Built via [`inboxly_core::reply::build_references_chain`] from
    /// the parent's existing `References` plus its `Message-ID`.
    pub references: Option<String>,

    // -- Lifecycle bookkeeping --
    /// Set by any field-change handler. Cleared by the Phase 10 auto-save
    /// bridge after a successful `Store::update_draft` commit, but only
    /// if `save_generation` still matches the captured snapshot
    /// (Issue 1.8 stale-result guard).
    pub dirty: bool,
    /// Incremented on every field change. The Phase 10 auto-save bridge
    /// captures this at save-trigger time and only clears `dirty` if the
    /// captured value still matches after the save commit.
    pub save_generation: u64,

    // -- Send state (Gemini G9 two-phase commit) --
    /// Current send pipeline state.
    pub send_state: ComposeSendState,

    // -- Picker bridge trigger (M35 Phase 11) --
    /// Counter bumped by `Message::ComposeAttachFile`. The Phase 11
    /// picker bridge in `components::app` watches this and triggers
    /// `rfd::AsyncFileDialog::pick_file()` when it changes. Using a
    /// counter (rather than a `bool`) so two consecutive picks of the
    /// same file count as two distinct events — `bool` would coalesce
    /// them and the second pick would never spawn the dialog.
    ///
    /// `wrapping_add(1)` is used in the handler so the bridge keeps
    /// firing across the (effectively unreachable) `u64::MAX` boundary
    /// without panicking. Initial value is `0`; the bridge skips the
    /// initial render so the dialog does not pop on app start.
    pub attach_picker_counter: u64,

    // -- Reply prefill two-step dispatch (M36 Phase 8) --
    /// Sentinel set by [`crate::app::Message::OpenComposeReply`] to ask
    /// the reply-prefill bridge in `inboxly-ui::components::app` to
    /// load the original message and build a Reply/ReplyAll/Forward
    /// `ComposeState`. The bridge watches a `use_memo` over this field;
    /// when it transitions from `None` to `Some`, the bridge spawns a
    /// task that calls `ThreadReader::load_email`, then dispatches
    /// [`crate::app::Message::ComposeReplyReady`] (success) or
    /// [`crate::app::Message::ComposeReplyFailed`] (error). Both
    /// terminal handlers clear this field back to `None`.
    ///
    /// Two-step dispatch is required because the click handler runs
    /// inside the synchronous `Inboxly::update` loop, which cannot call
    /// the `!Send + !Sync` `ThreadReader` (which holds a SQLite
    /// connection) without blocking the event loop on disk I/O — and
    /// because `Inboxly` itself does not own a `ThreadReader` handle in
    /// every test fixture. The bridge picks the handle up via
    /// `use_context` / `peek` at task spawn time.
    pub pending_reply: Option<(String, inboxly_core::ComposeMode)>,

    /// True while the reply-prefill task is running. Set synchronously
    /// by [`crate::app::Message::OpenComposeReply`] alongside
    /// `pending_reply`, cleared by both
    /// [`crate::app::Message::ComposeReplyReady`] and
    /// [`crate::app::Message::ComposeReplyFailed`]. Used by the compose
    /// view (Phase 11+) to render a "Loading original..." sentinel.
    pub loading_reply: bool,

    // -- Explicit save bridge trigger (M36 Phase 5) --
    /// Counter bumped by [`crate::app::Message::ComposeSaveDraft`] (the
    /// "Save Draft" button) and by the `Navigate` handler when it
    /// detects dirty compose state being navigated away from. The
    /// Phase 5 explicit-save bridge in `components::app` watches this
    /// counter and fires SQLite + Maildir `.Drafts/` writes
    /// synchronously (no 30 s timer like the auto-save bridge).
    ///
    /// Counter (rather than `bool`) for the same reason as
    /// `attach_picker_counter`: two rapid Save Draft clicks must
    /// produce two distinct bridge fires, which a `bool` would
    /// coalesce. `wrapping_add(1)` so the bridge survives the
    /// (unreachable) `u64::MAX` boundary. Initial value is `0`; the
    /// bridge skips the initial render so the save logic does not run
    /// on app start before any draft exists.
    pub explicit_save_counter: u64,
}

/// Two-phase commit state for the SMTP send pipeline.
///
/// On send success, the state transitions to `Sent { dismiss_pending: true }`
/// and the compose view shows a "Sent — dismiss?" overlay. The user clicks
/// dismiss to clear the compose and return to the inbox. This preserves
/// concurrent edits during the send window — the user can immediately
/// compose again if needed (Gemini G9).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum ComposeSendState {
    /// No send in progress.
    #[default]
    Idle,
    /// Send is in flight; field-change handlers must not mutate state.
    Sending,
    /// SMTP send succeeded; waiting for the user to dismiss the
    /// "Sent" overlay before clearing compose state.
    Sent {
        /// True until the user clicks the dismiss button.
        dismiss_pending: bool,
    },
    /// SMTP send failed; the error message is shown to the user.
    Failed {
        /// Human-readable failure reason (already redacted of credentials).
        error: String,
    },
}

impl Default for ComposeState {
    fn default() -> Self {
        Self::new()
    }
}

impl ComposeState {
    /// Create empty compose state (no active draft, all defaults).
    #[must_use]
    pub fn new() -> Self {
        Self {
            draft_id: None,
            subject: String::new(),
            from_account_index: 0,
            to: Vec::new(),
            cc: Vec::new(),
            bcc: Vec::new(),
            to_input: String::new(),
            cc_input: String::new(),
            bcc_input: String::new(),
            show_cc_bcc: false,
            body_markdown: String::new(),
            show_preview: false,
            attachments: Vec::new(),
            mode: ComposeMode::New,
            in_reply_to: None,
            references: None,
            dirty: false,
            save_generation: 0,
            send_state: ComposeSendState::Idle,
            attach_picker_counter: 0,
            explicit_save_counter: 0,
            pending_reply: None,
            loading_reply: false,
        }
    }

    /// Bump the save generation and mark the draft dirty. Called by every
    /// field-change handler so the Phase 10 auto-save bridge can detect
    /// concurrent edits via the snapshot/check pattern.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
        self.save_generation = self.save_generation.wrapping_add(1);
    }

    /// True if the compose form has the minimum data needed to send: at
    /// least one To recipient AND a non-empty subject AND not currently
    /// mid-send. Used by Phase 8 to enable/disable the Send button.
    #[must_use]
    pub fn can_send(&self) -> bool {
        !self.to.is_empty()
            && !self.subject.trim().is_empty()
            && self.send_state == ComposeSendState::Idle
    }
}
