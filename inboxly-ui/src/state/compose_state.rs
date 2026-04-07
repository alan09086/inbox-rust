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
            dirty: false,
            save_generation: 0,
            send_state: ComposeSendState::Idle,
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
