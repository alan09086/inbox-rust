//! Compose view: full-screen email composer.
//!
//! M35 Phase 8. Renders the entire compose UI from `Inboxly::compose`
//! state. All input changes dispatch `Compose*` Messages — this component
//! has zero side effects of its own. The async I/O bridges (auto-save,
//! attachment picker, send) live in `components/app.rs` (Phases 10/11/12).
//!
//! ## Composition
//!
//! - [`ComposeView`] — top-level component reading from
//!   `Signal<Inboxly>` and rendering the header, form, body, attachments
//!   and footer in one scrollable column.
//! - [`RecipientChip`] — a single chip in the To/Cc/Bcc rows with an
//!   `\u{00D7}` remove button.
//! - [`AttachmentChip`] — a single chip in the attachment row showing
//!   filename + human-readable size + remove button.
//! - [`AccountPickerDropdown`] — a `<select>` for the FROM account when
//!   the user has more than one configured account; a static label for
//!   single-account setups.
//!
//! `Signal<Inboxly>` is `Copy`, so each event closure captures it by
//! value via `move |...|` — there is no need for per-closure clones the
//! way `Arc` handles would require. The single `let mut app_state` at
//! the top of each component is the only handle we need.

use std::sync::Arc;

use dioxus::prelude::*;

use crate::app::{Inboxly, Message, RecipientField};
use crate::markdown_preview::render_markdown_preview;
use crate::state::ComposeSendState;
use inboxly_core::{AttachmentDraft, Contact, parse_address_list};

/// Full-screen compose view rendered when `active_view == Compose`.
///
/// Reads every field of `Inboxly::compose` and dispatches `Compose*`
/// `Message` variants on user input. The send button is disabled
/// unless [`crate::state::ComposeState::can_send`] returns `true`
/// (Phase 6 helper: at least one To recipient + non-empty subject +
/// `send_state == Idle`).
#[component]
pub fn ComposeView() -> Element {
    let mut app_state = use_context::<Signal<Inboxly>>();

    // Snapshot every field we render so the read guard can be dropped
    // before any closure mutates app_state. Recipient/attachment Vecs
    // are Arc<…> per Phase 6 Issue 4.2 — these clones are refcount
    // bumps, not deep copies.
    let (
        subject,
        to,
        cc,
        bcc,
        to_input,
        cc_input,
        bcc_input,
        body_markdown,
        attachments,
        show_cc_bcc,
        show_preview,
        send_state,
        from_account_index,
        can_send,
        accounts,
    ) = {
        let state = app_state.read();
        let compose = &state.compose;
        let accounts: Vec<(usize, String, String)> = state
            .accounts
            .iter()
            .enumerate()
            .map(|(i, a)| (i, a.email.clone(), a.display_name.clone()))
            .collect();
        (
            compose.subject.clone(),
            compose.to.clone(),
            compose.cc.clone(),
            compose.bcc.clone(),
            compose.to_input.clone(),
            compose.cc_input.clone(),
            compose.bcc_input.clone(),
            compose.body_markdown.clone(),
            compose.attachments.clone(),
            compose.show_cc_bcc,
            compose.show_preview,
            compose.send_state.clone(),
            compose.from_account_index,
            compose.can_send(),
            accounts,
        )
    };

    // Send-state derived fragments. Computed up front so the rsx! tree
    // below stays linear and easy to read.
    let sent_overlay = if matches!(
        send_state,
        ComposeSendState::Sent {
            dismiss_pending: true
        }
    ) {
        rsx! {
            div {
                class: "compose-error-banner",
                style: "background: var(--active-bg); color: var(--accent-blue); border-left-color: var(--accent-blue);",
                span { class: "thread-detail-error-icon", "\u{2713}" } // ✓
                span { class: "thread-detail-error-text", "Sent" }
                button {
                    class: "compose-discard-button",
                    onclick: move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        app_state.write().update(Message::ComposeDismissSentNotice);
                    },
                    "Dismiss"
                }
            }
        }
    } else {
        rsx! {}
    };

    let error_banner = if let ComposeSendState::Failed { error } = &send_state {
        let error_msg = error.clone();
        rsx! {
            div {
                class: "compose-error-banner",
                role: "alert",
                span { class: "thread-detail-error-icon", "\u{26A0}" } // ⚠
                span { class: "thread-detail-error-text", "{error_msg}" }
            }
        }
    } else {
        rsx! {}
    };

    let body_section = if show_preview {
        let html = render_markdown_preview(&body_markdown);
        rsx! {
            div {
                class: "compose-body-preview",
                dangerous_inner_html: "{html}",
            }
        }
    } else {
        rsx! {
            textarea {
                class: "compose-body-textarea",
                value: "{body_markdown}",
                placeholder: "Compose your message in Markdown\u{2026}",
                oninput: move |evt: Event<FormData>| {
                    app_state.write().update(Message::ComposeBodyChanged(evt.value()));
                },
            }
        }
    };

    let send_label = if matches!(send_state, ComposeSendState::Sending) {
        "Sending\u{2026}"
    } else {
        "Send"
    };
    let toggle_cc_bcc_label = if show_cc_bcc {
        "Hide Cc/Bcc"
    } else {
        "Show Cc/Bcc"
    };
    let toggle_preview_label = if show_preview { "Edit" } else { "Preview" };

    rsx! {
        div {
            class: "compose-view",

            // -- Header: back arrow + title + account picker --
            div {
                class: "compose-header",
                button {
                    class: "compose-discard-button",
                    aria_label: "Close compose",
                    onclick: move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        app_state.write().update(Message::CloseCompose);
                    },
                    "\u{2190} Back" // ←
                }
                span {
                    style: "font-size: 16px; font-weight: 500; color: var(--text-primary); flex: 1;",
                    "Compose"
                }
                AccountPickerDropdown {
                    accounts: accounts.clone(),
                    selected_index: from_account_index,
                }
            }

            {sent_overlay}
            {error_banner}

            // -- Form: recipients + subject + body + attachments --
            div {
                class: "compose-form",

                // -- To recipients --
                div {
                    class: "compose-field",
                    span {
                        style: "font-size: 12px; color: var(--text-secondary);",
                        "To"
                    }
                    div {
                        class: "compose-recipients",
                        for (idx, contact) in to.iter().enumerate() {
                            RecipientChip {
                                key: "to-{idx}",
                                contact: Arc::clone(contact),
                                field: RecipientField::To,
                                index: idx,
                            }
                        }
                        input {
                            r#type: "text",
                            value: "{to_input}",
                            style: "flex: 1; min-width: 100px; border: none; background: transparent; color: var(--text-primary); font-size: 14px; outline: none; padding: 4px 0;",
                            placeholder: "name@example.com",
                            oninput: move |evt: Event<FormData>| {
                                app_state.write().update(Message::ComposeToInputChanged(evt.value()));
                            },
                            onkeydown: move |evt: KeyboardEvent| {
                                if is_recipient_commit_key(&evt.key()) {
                                    evt.prevent_default();
                                    let current = app_state.read().compose.to_input.clone();
                                    if let Some(parsed) = try_parse_recipient(&current) {
                                        let mut s = app_state.write();
                                        s.update(Message::ComposeAddRecipient {
                                            field: RecipientField::To,
                                            contact: parsed,
                                        });
                                        s.update(Message::ComposeToInputChanged(String::new()));
                                    }
                                }
                            },
                        }
                    }
                    button {
                        style: "background: none; border: none; color: var(--accent-blue); font-size: 12px; cursor: pointer; align-self: flex-start; padding: 0;",
                        onclick: move |evt: Event<MouseData>| {
                            evt.stop_propagation();
                            app_state.write().update(Message::ComposeToggleCcBcc);
                        },
                        "{toggle_cc_bcc_label}"
                    }
                }

                // -- Cc and Bcc (collapsible) --
                if show_cc_bcc {
                    div {
                        class: "compose-field",
                        span {
                            style: "font-size: 12px; color: var(--text-secondary);",
                            "Cc"
                        }
                        div {
                            class: "compose-recipients",
                            for (idx, contact) in cc.iter().enumerate() {
                                RecipientChip {
                                    key: "cc-{idx}",
                                    contact: Arc::clone(contact),
                                    field: RecipientField::Cc,
                                    index: idx,
                                }
                            }
                            input {
                                r#type: "text",
                                value: "{cc_input}",
                                style: "flex: 1; min-width: 100px; border: none; background: transparent; color: var(--text-primary); font-size: 14px; outline: none; padding: 4px 0;",
                                placeholder: "name@example.com",
                                oninput: move |evt: Event<FormData>| {
                                    app_state.write().update(Message::ComposeCcInputChanged(evt.value()));
                                },
                                onkeydown: move |evt: KeyboardEvent| {
                                    if is_recipient_commit_key(&evt.key()) {
                                        evt.prevent_default();
                                        let current = app_state.read().compose.cc_input.clone();
                                        if let Some(parsed) = try_parse_recipient(&current) {
                                            let mut s = app_state.write();
                                            s.update(Message::ComposeAddRecipient {
                                                field: RecipientField::Cc,
                                                contact: parsed,
                                            });
                                            s.update(Message::ComposeCcInputChanged(String::new()));
                                        }
                                    }
                                },
                            }
                        }
                    }
                    div {
                        class: "compose-field",
                        span {
                            style: "font-size: 12px; color: var(--text-secondary);",
                            "Bcc"
                        }
                        div {
                            class: "compose-recipients",
                            for (idx, contact) in bcc.iter().enumerate() {
                                RecipientChip {
                                    key: "bcc-{idx}",
                                    contact: Arc::clone(contact),
                                    field: RecipientField::Bcc,
                                    index: idx,
                                }
                            }
                            input {
                                r#type: "text",
                                value: "{bcc_input}",
                                style: "flex: 1; min-width: 100px; border: none; background: transparent; color: var(--text-primary); font-size: 14px; outline: none; padding: 4px 0;",
                                placeholder: "name@example.com",
                                oninput: move |evt: Event<FormData>| {
                                    app_state.write().update(Message::ComposeBccInputChanged(evt.value()));
                                },
                                onkeydown: move |evt: KeyboardEvent| {
                                    if is_recipient_commit_key(&evt.key()) {
                                        evt.prevent_default();
                                        let current = app_state.read().compose.bcc_input.clone();
                                        if let Some(parsed) = try_parse_recipient(&current) {
                                            let mut s = app_state.write();
                                            s.update(Message::ComposeAddRecipient {
                                                field: RecipientField::Bcc,
                                                contact: parsed,
                                            });
                                            s.update(Message::ComposeBccInputChanged(String::new()));
                                        }
                                    }
                                },
                            }
                        }
                    }
                }

                // -- Subject --
                div {
                    class: "compose-field",
                    span {
                        style: "font-size: 12px; color: var(--text-secondary);",
                        "Subject"
                    }
                    input {
                        class: "compose-subject",
                        r#type: "text",
                        value: "{subject}",
                        placeholder: "Subject",
                        oninput: move |evt: Event<FormData>| {
                            app_state.write().update(Message::ComposeSubjectChanged(evt.value()));
                        },
                    }
                }

                // -- Body (textarea or rendered preview) --
                div {
                    class: "compose-field",
                    div {
                        style: "display: flex; align-items: center; justify-content: space-between;",
                        span {
                            style: "font-size: 12px; color: var(--text-secondary);",
                            "Body (Markdown)"
                        }
                        button {
                            class: "compose-toggle-preview",
                            onclick: move |evt: Event<MouseData>| {
                                evt.stop_propagation();
                                app_state.write().update(Message::ComposeTogglePreview);
                            },
                            "{toggle_preview_label}"
                        }
                    }
                    div { class: "compose-body", {body_section} }
                }

                // -- Attachments (only when non-empty) --
                if !attachments.is_empty() {
                    div {
                        class: "compose-field",
                        span {
                            style: "font-size: 12px; color: var(--text-secondary);",
                            "Attachments"
                        }
                        div {
                            class: "compose-attachments",
                            for (idx, att) in attachments.iter().enumerate() {
                                AttachmentChip {
                                    key: "att-{idx}",
                                    attachment: Arc::clone(att),
                                    index: idx,
                                }
                            }
                        }
                    }
                }
            }

            // -- Footer: Attach / Save / Discard / Send --
            div {
                class: "compose-footer",
                button {
                    class: "compose-discard-button",
                    onclick: move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        app_state.write().update(Message::ComposeAttachFile);
                    },
                    "\u{1F4CE} Attach" // 📎
                }
                button {
                    class: "compose-discard-button",
                    onclick: move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        app_state.write().update(Message::ComposeSaveDraft);
                    },
                    "Save Draft"
                }
                button {
                    class: "compose-discard-button",
                    style: "color: var(--menu-destructive-text); border-color: var(--menu-destructive-text);",
                    onclick: move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        app_state.write().update(Message::ComposeDiscardDraft);
                    },
                    "Discard"
                }
                button {
                    class: "compose-send-button",
                    disabled: !can_send,
                    onclick: move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        app_state.write().update(Message::ComposeSendDraft);
                    },
                    "{send_label}"
                }
            }
        }
    }
}

/// True if `key` is the Enter or comma key — the two recipient
/// commit triggers. Comma arrives as `Key::Character(",")` per the
/// W3C UI Events spec.
fn is_recipient_commit_key(key: &Key) -> bool {
    if matches!(key, Key::Enter) {
        return true;
    }
    if let Key::Character(s) = key {
        return s == ",";
    }
    false
}

/// Try to parse a recipient input string into a [`Contact`].
///
/// Returns `None` when the trimmed input is empty or doesn't contain
/// an `@` after parsing — the recipient input field then keeps the
/// raw text so the user can correct it.
fn try_parse_recipient(input: &str) -> Option<Contact> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parsed = parse_address_list(trimmed).into_iter().next()?;
    if !parsed.address.contains('@') {
        return None;
    }
    Some(Contact::new(
        parsed.name.unwrap_or_default(),
        parsed.address,
    ))
}

/// A single recipient chip in the To/Cc/Bcc row.
///
/// The remove button dispatches [`Message::ComposeRemoveRecipient`]
/// with the row's `field` and the chip's index. Wrapped in `Arc`
/// (per Phase 6 Issue 4.2) so the per-render clone in the parent
/// `for` loop is a refcount bump rather than a deep `Contact` copy.
#[component]
fn RecipientChip(contact: Arc<Contact>, field: RecipientField, index: usize) -> Element {
    let mut app_state = use_context::<Signal<Inboxly>>();
    let display = if contact.name.is_empty() {
        contact.address.clone()
    } else {
        format!("{} <{}>", contact.name, contact.address)
    };
    rsx! {
        span {
            class: "recipient-chip",
            "{display}"
            button {
                class: "recipient-chip-remove",
                aria_label: "Remove recipient",
                onclick: move |evt: Event<MouseData>| {
                    evt.stop_propagation();
                    app_state
                        .write()
                        .update(Message::ComposeRemoveRecipient { field, index });
                },
                "\u{00D7}" // ×
            }
        }
    }
}

/// A single attachment chip showing filename + human-readable size.
///
/// The remove button dispatches [`Message::ComposeRemoveAttachment`].
/// Wrapped in `Arc` so per-render clones are refcount bumps.
#[component]
fn AttachmentChip(attachment: Arc<AttachmentDraft>, index: usize) -> Element {
    let mut app_state = use_context::<Signal<Inboxly>>();
    let filename = attachment.filename.clone();
    let size_label = format_size(attachment.size_bytes);
    rsx! {
        span {
            class: "attachment-chip",
            span { class: "attachment-chip-icon", "\u{1F4CE}" } // 📎
            span { class: "attachment-chip-filename", "{filename}" }
            span { class: "attachment-chip-size", "{size_label}" }
            button {
                class: "attachment-chip-remove",
                aria_label: "Remove attachment",
                onclick: move |evt: Event<MouseData>| {
                    evt.stop_propagation();
                    app_state
                        .write()
                        .update(Message::ComposeRemoveAttachment(index));
                },
                "\u{00D7}" // ×
            }
        }
    }
}

/// Format bytes as a human-readable size (e.g., "1.5 KB", "12.3 MB").
///
/// Mirrors the existing `app::format_size` helper.
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// FROM-account picker dropdown.
///
/// Shows a `<select>` when there is more than one configured account;
/// for single-account setups it falls back to a static label so the
/// user isn't presented with a one-option dropdown. The selection
/// dispatches [`Message::ComposeFromChanged`] with the chosen index.
#[component]
fn AccountPickerDropdown(accounts: Vec<(usize, String, String)>, selected_index: usize) -> Element {
    let mut app_state = use_context::<Signal<Inboxly>>();
    if accounts.len() <= 1 {
        let email = accounts
            .first()
            .map(|(_, e, _)| e.clone())
            .unwrap_or_default();
        return rsx! {
            span {
                style: "font-size: 12px; color: var(--text-secondary);",
                "From: {email}"
            }
        };
    }
    rsx! {
        select {
            style: "padding: 4px 8px; border: 1px solid var(--divider-color); border-radius: 4px; background: var(--surface-color); color: var(--text-primary); font-size: 13px;",
            onchange: move |evt: Event<FormData>| {
                if let Ok(idx) = evt.value().parse::<usize>() {
                    app_state
                        .write()
                        .update(Message::ComposeFromChanged { account_index: idx });
                }
            },
            for (idx, email, name) in accounts.iter().cloned() {
                {
                    let label = if name.is_empty() {
                        email.clone()
                    } else {
                        format!("{name} <{email}>")
                    };
                    rsx! {
                        option {
                            key: "acct-{idx}",
                            value: "{idx}",
                            selected: idx == selected_index,
                            "{label}"
                        }
                    }
                }
            }
        }
    }
}
