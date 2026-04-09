//! Pure helpers for reply/forward composition.
//!
//! This module is deliberately I/O-free, async-free, and state-free: every
//! function takes primitive inputs and returns a value. Date formatting is
//! the caller's responsibility (so this module does not depend on
//! `chrono`), and any model wrapping (`LoadedEmail`, `EmailRow`) is done at
//! the call site in `inboxly-ui`. The goal is that every helper here is
//! trivially unit-testable without fixtures.
//!
//! All conventions follow Gmail's behaviour:
//!
//! - Subject prefixes: English-only `Re:` / `Fwd:` (with `RE:`, `FW:`,
//!   `FWD:`, `Re :` normalised). Localised prefixes (`AW:`, `Rép:`, `SV:`)
//!   are explicitly out of scope.
//! - Reply quote: `On <date>, <from> wrote:` followed by `> ` prefixed
//!   body with nesting preserved.
//! - Forward quote: ten-dash separator + From/Date/Subject/To header
//!   block + body without quote prefix.
//! - References chain: parent References + parent Message-ID, pruned to
//!   "keep first 3 + last 5" if the result would exceed 500 chars (RFC
//!   5322 §2.1.1 line-length safety margin, JWZ threading preserved).
//! - ReplyAll recipients: case-insensitive dedup via
//!   [`Contact::same_address`], with the reply-to-self edge case handled
//!   so users can continue threads they themselves started.

use crate::Contact;

/// Maximum byte length for the constructed References chain before pruning.
///
/// RFC 5322 §2.1.1 caps a single header line at 998 chars; we use 500 as a
/// safety margin so the assembled `References:` line still fits even after
/// folding-whitespace and the `References: ` prefix itself.
const REFERENCES_MAX_LEN: usize = 500;

/// When pruning, keep this many tokens from the start of the chain.
///
/// The first three Message-IDs anchor JWZ-style threading: thread root +
/// early context. Discarding them would orphan the reply from its thread
/// in clients that walk the References chain in order.
const REFERENCES_KEEP_FIRST: usize = 3;

/// When pruning, keep this many tokens from the end of the chain.
///
/// The last five Message-IDs are the most recent turns in the conversation
/// and matter most for reply context.
const REFERENCES_KEEP_LAST: usize = 5;

// ---------------------------------------------------------------------------
// Subject normalisation
// ---------------------------------------------------------------------------

/// Normalise a reply subject line (Gmail-compatible, English only).
///
/// Behaviour:
/// - Empty / whitespace-only input → `"Re:"`
/// - `"Re:"` (any case, optional space before colon) → `"Re:"` form
/// - `"Re: Re: foo"` → `"Re: foo"` (collapses adjacent duplicate prefixes)
/// - `"Fwd: foo"` → `"Re: foo"` (strips Fwd, replaces with Re)
/// - `"Fwd: Re: foo"` → `"Re: foo"` (strips both, adds Re back)
/// - Plain `"foo"` → `"Re: foo"`
///
/// Internal whitespace in the body is preserved; only leading/trailing
/// whitespace is trimmed before prefix detection.
#[must_use]
pub fn subject_for_reply(original: &str) -> String {
    let stripped = strip_reply_forward_prefixes(original.trim());
    if stripped.is_empty() {
        "Re:".to_string()
    } else {
        format!("Re: {stripped}")
    }
}

/// Normalise a forward subject line (Gmail-compatible, English only).
///
/// Behaviour:
/// - Empty / whitespace-only input → `"Fwd:"`
/// - `"Fwd:"` / `"FW:"` / `"FWD:"` / `"fwd:"` (with optional space before
///   colon) → already-prefixed, normalise prefix to `"Fwd:"` and pass body
///   through unchanged
/// - `"Re: foo"` → `"Fwd: Re: foo"` (preserves the Re inside the Fwd, per
///   Gmail convention)
/// - Plain `"foo"` → `"Fwd: foo"`
#[must_use]
pub fn subject_for_forward(original: &str) -> String {
    let trimmed = original.trim();
    if trimmed.is_empty() {
        return "Fwd:".to_string();
    }
    // If already a Fwd: prefix (in any casing), just normalise the prefix.
    if let Some(rest) = strip_one_forward_prefix(trimmed) {
        let rest = rest.trim_start();
        if rest.is_empty() {
            return "Fwd:".to_string();
        }
        return format!("Fwd: {rest}");
    }
    format!("Fwd: {trimmed}")
}

/// Strip leading `Re:` and `Fwd:` prefixes (in any combination, any
/// casing, with optional space before the colon) until none remain.
///
/// Returns the body with internal whitespace preserved but with the
/// stripped prefixes removed.
fn strip_reply_forward_prefixes(s: &str) -> String {
    let mut current = s.trim().to_string();
    loop {
        if let Some(rest) = strip_one_reply_prefix(&current) {
            current = rest.trim_start().to_string();
            continue;
        }
        if let Some(rest) = strip_one_forward_prefix(&current) {
            current = rest.trim_start().to_string();
            continue;
        }
        break;
    }
    current
}

/// If `s` starts with a `Re:` prefix (case-insensitive, optional space
/// before colon), return the remainder. Otherwise `None`.
fn strip_one_reply_prefix(s: &str) -> Option<String> {
    strip_prefix_with_optional_space(s, "re")
}

/// If `s` starts with a `Fwd:` / `Fw:` / `FWD:` prefix (case-insensitive,
/// optional space before colon), return the remainder. Otherwise `None`.
fn strip_one_forward_prefix(s: &str) -> Option<String> {
    strip_prefix_with_optional_space(s, "fwd").or_else(|| strip_prefix_with_optional_space(s, "fw"))
}

/// Try to strip `<keyword>` (case-insensitive) followed by optional
/// whitespace and a colon from the start of `s`.
fn strip_prefix_with_optional_space(s: &str, keyword: &str) -> Option<String> {
    let lower = s.to_ascii_lowercase();
    let lower_bytes = lower.as_bytes();
    let kw_bytes = keyword.as_bytes();
    if !lower_bytes.starts_with(kw_bytes) {
        return None;
    }
    // Walk past the keyword and any spaces/tabs, then expect a colon.
    let mut idx = kw_bytes.len();
    while idx < lower_bytes.len() && (lower_bytes[idx] == b' ' || lower_bytes[idx] == b'\t') {
        idx += 1;
    }
    if idx >= lower_bytes.len() || lower_bytes[idx] != b':' {
        return None;
    }
    idx += 1;
    // Slice the original (not the lowercased copy) on the same byte index.
    // Safe because we only walked past ASCII bytes.
    Some(s.get(idx..)?.to_string())
}

// ---------------------------------------------------------------------------
// References chain
// ---------------------------------------------------------------------------

/// Build the References header chain for a reply.
///
/// Concatenates the parent's References header (if any) with the parent's
/// Message-ID, separated by spaces. If the resulting chain would exceed
/// [`REFERENCES_MAX_LEN`] bytes, prunes to "keep first
/// [`REFERENCES_KEEP_FIRST`] + last [`REFERENCES_KEEP_LAST`]" tokens — the
/// first preserve JWZ threading roots, the last preserve recent context.
///
/// This keeps the assembled `References:` header under the RFC 5322
/// §2.1.1 998-character line limit even after folding-whitespace.
///
/// `original_message_id` is included verbatim (no `<>` wrapping is added —
/// callers are expected to pass it in the form they want stored).
#[must_use]
pub fn build_references_chain(
    original_references: Option<&str>,
    original_message_id: &str,
) -> String {
    let trimmed_msgid = original_message_id.trim();
    let chain = match original_references {
        Some(refs) if !refs.trim().is_empty() => {
            if trimmed_msgid.is_empty() {
                refs.trim().to_string()
            } else {
                format!("{} {trimmed_msgid}", refs.trim())
            }
        }
        _ => trimmed_msgid.to_string(),
    };

    if chain.len() <= REFERENCES_MAX_LEN {
        return chain;
    }

    // Prune: split into space-separated tokens, keep first N + last M.
    let tokens: Vec<&str> = chain.split_whitespace().collect();
    if tokens.len() <= REFERENCES_KEEP_FIRST + REFERENCES_KEEP_LAST {
        // Nothing to prune (every token is already kept).
        return tokens.join(" ");
    }
    let mut kept: Vec<&str> = Vec::with_capacity(REFERENCES_KEEP_FIRST + REFERENCES_KEEP_LAST);
    kept.extend(tokens.iter().take(REFERENCES_KEEP_FIRST));
    kept.extend(tokens.iter().skip(tokens.len() - REFERENCES_KEEP_LAST));
    kept.join(" ")
}

// ---------------------------------------------------------------------------
// Quote formatting
// ---------------------------------------------------------------------------

/// Format a reply quote block: attribution line + `> ` prefixed body.
///
/// Produces output of the form:
///
/// ```text
/// On <date_formatted>, <Name> <addr@host> wrote:
/// > first line of original
/// > second line of original
/// > > already-quoted lines get nested deeper
/// ```
///
/// `date_formatted` should be a human-readable date string (e.g.
/// `"Thu, 7 Apr 2026 at 14:32"`); the caller owns date formatting so this
/// module stays independent of `chrono`. An empty body produces just the
/// attribution line with no `>` block.
#[must_use]
pub fn format_reply_quote(from: &Contact, date_formatted: &str, body: &str) -> String {
    let attribution = format!("On {date_formatted}, {from} wrote:");
    if body.is_empty() {
        return attribution;
    }
    let mut out = String::with_capacity(attribution.len() + body.len() + 16);
    out.push_str(&attribution);
    for line in body.split('\n') {
        out.push('\n');
        // Already-quoted lines get an additional `> ` prefix so nesting
        // is preserved (`> >`, `> > >`, etc).
        if line.starts_with('>') {
            out.push_str("> ");
            out.push_str(line);
        } else if line.is_empty() {
            // Preserve blank lines but emit just `>` (no trailing space).
            out.push('>');
        } else {
            out.push_str("> ");
            out.push_str(line);
        }
    }
    out
}

/// Format a forward quote block: separator + header block + raw body.
///
/// Produces output of the form:
///
/// ```text
/// ---------- Forwarded message ----------
/// From: <Name> <addr@host>
/// Date: <date_formatted>
/// Subject: <subject>
/// To: <Name1> <addr1@host>, <Name2> <addr2@host>
///
/// <body unchanged>
/// ```
///
/// The body is **not** quote-prefixed: forwards preserve the original as-is
/// (Gmail convention). Header values are accepted as parameters so this
/// helper stays pure — no I/O, no formatting dependencies.
#[must_use]
pub fn format_forward_quote(
    from: &Contact,
    date_formatted: &str,
    subject: &str,
    to_list: &[Contact],
    body: &str,
) -> String {
    let to_joined = to_list
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    let mut out = String::with_capacity(body.len() + 256);
    out.push_str("---------- Forwarded message ----------\n");
    out.push_str(&format!("From: {from}\n"));
    out.push_str(&format!("Date: {date_formatted}\n"));
    out.push_str(&format!("Subject: {subject}\n"));
    out.push_str(&format!("To: {to_joined}\n"));
    out.push('\n');
    out.push_str(body);
    out
}

// ---------------------------------------------------------------------------
// ReplyAll recipient computation
// ---------------------------------------------------------------------------

/// Compute the (To, Cc) tuple for a ReplyAll.
///
/// Normal case (`original_from != user_email`):
/// - To = `[original_from]`
/// - Cc = `(original_to ∪ original_cc) \ user_email`, deduped
///   case-insensitively via [`Contact::same_address`]
///
/// Reply-to-self edge case (`original_from == user_email`): the user sent
/// the original message and is now continuing the thread. We must NOT
/// reply to ourselves; instead, preserve the original recipients:
/// - To = `original_to \ user_email`
/// - Cc = `original_cc \ user_email`
///
/// Both lists are deduped against each other so a contact never appears
/// in both To and Cc, and the user themselves is always excluded.
#[must_use]
pub fn reply_all_recipients(
    original_from: &Contact,
    original_to: &[Contact],
    original_cc: &[Contact],
    user_email: &str,
) -> (Vec<Contact>, Vec<Contact>) {
    let user_is_sender = original_from.same_address(user_email);

    let (to, cc_source): (Vec<Contact>, Vec<Contact>) = if user_is_sender {
        // Reply-to-self: preserve the original To, drop user from both lists.
        let to: Vec<Contact> = original_to
            .iter()
            .filter(|c| !c.same_address(user_email))
            .cloned()
            .collect();
        let cc: Vec<Contact> = original_cc
            .iter()
            .filter(|c| !c.same_address(user_email))
            .cloned()
            .collect();
        (to, cc)
    } else {
        // Normal: To = [from], Cc = (orig.To ∪ orig.Cc) \ user.
        let to = vec![original_from.clone()];
        let mut cc: Vec<Contact> = Vec::new();
        for source in original_to.iter().chain(original_cc.iter()) {
            if source.same_address(user_email) {
                continue;
            }
            cc.push(source.clone());
        }
        (to, cc)
    };

    // Dedup `to` against itself (preserves order, keeps first occurrence).
    let to = dedup_contacts(&to);
    // Dedup `cc` against itself, then remove anything already in `to`.
    let cc = dedup_contacts(&cc_source);
    let cc: Vec<Contact> = cc
        .into_iter()
        .filter(|c| !to.iter().any(|t| t.same_address(&c.address)))
        .collect();
    (to, cc)
}

/// Order-preserving case-insensitive dedup over a slice of `Contact`.
fn dedup_contacts(input: &[Contact]) -> Vec<Contact> {
    let mut out: Vec<Contact> = Vec::with_capacity(input.len());
    for c in input {
        if !out.iter().any(|existing| existing.same_address(&c.address)) {
            out.push(c.clone());
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- subject_for_reply ---

    #[test]
    fn reply_subject_plain() {
        assert_eq!(subject_for_reply("hello"), "Re: hello");
    }

    #[test]
    fn reply_subject_already_re() {
        assert_eq!(subject_for_reply("Re: hello"), "Re: hello");
    }

    #[test]
    fn reply_subject_uppercase_re() {
        assert_eq!(subject_for_reply("RE: hello"), "Re: hello");
    }

    #[test]
    fn reply_subject_re_space_before_colon() {
        assert_eq!(subject_for_reply("Re : hello"), "Re: hello");
    }

    #[test]
    fn reply_subject_double_re_collapses() {
        assert_eq!(subject_for_reply("Re: Re: foo"), "Re: foo");
    }

    #[test]
    fn reply_subject_fwd_becomes_re() {
        assert_eq!(subject_for_reply("Fwd: foo"), "Re: foo");
    }

    #[test]
    fn reply_subject_fwd_re_collapses_to_re() {
        assert_eq!(subject_for_reply("Fwd: Re: foo"), "Re: foo");
    }

    #[test]
    fn reply_subject_empty_input() {
        assert_eq!(subject_for_reply(""), "Re:");
        assert_eq!(subject_for_reply("   "), "Re:");
    }

    #[test]
    fn reply_subject_preserves_internal_whitespace() {
        assert_eq!(
            subject_for_reply("hello  world"),
            "Re: hello  world",
            "internal double space must be preserved",
        );
    }

    // --- subject_for_forward ---

    #[test]
    fn forward_subject_plain() {
        assert_eq!(subject_for_forward("hello"), "Fwd: hello");
    }

    #[test]
    fn forward_subject_already_fwd() {
        assert_eq!(subject_for_forward("Fwd: hello"), "Fwd: hello");
    }

    #[test]
    fn forward_subject_fw_normalises_to_fwd() {
        assert_eq!(subject_for_forward("FW: hello"), "Fwd: hello");
    }

    #[test]
    fn forward_subject_uppercase_fwd_normalises() {
        assert_eq!(subject_for_forward("FWD: hello"), "Fwd: hello");
    }

    #[test]
    fn forward_subject_lowercase_fwd_normalises() {
        assert_eq!(subject_for_forward("fwd: hello"), "Fwd: hello");
    }

    #[test]
    fn forward_subject_preserves_re_inside_fwd() {
        // Gmail convention: Fwd: Re: foo (don't strip the Re).
        assert_eq!(subject_for_forward("Re: foo"), "Fwd: Re: foo");
    }

    #[test]
    fn forward_subject_empty_input() {
        assert_eq!(subject_for_forward(""), "Fwd:");
        assert_eq!(subject_for_forward("   "), "Fwd:");
    }

    // --- build_references_chain ---

    #[test]
    fn references_no_parent_refs_just_msgid() {
        let chain = build_references_chain(None, "<msg-1@host>");
        assert_eq!(chain, "<msg-1@host>");
    }

    #[test]
    fn references_with_parent_refs() {
        let chain = build_references_chain(Some("<root@host> <a@host>"), "<parent@host>");
        assert_eq!(chain, "<root@host> <a@host> <parent@host>");
    }

    #[test]
    fn references_empty_parent_refs_just_msgid() {
        let chain = build_references_chain(Some("   "), "<msg-1@host>");
        assert_eq!(chain, "<msg-1@host>");
    }

    #[test]
    fn references_pruning_keep_first_3_last_5() {
        // Build a chain that's >500 chars: 20 long Message-IDs.
        let long_id = |n: usize| format!("<{:0>50}@host.example.com>", n);
        let mut tokens: Vec<String> = (0..20).map(long_id).collect();
        let parent_msgid = tokens.pop().expect("at least one token");
        let parent_refs = tokens.join(" ");
        let chain = build_references_chain(Some(&parent_refs), &parent_msgid);
        // Expect first 3 + last 5 = 8 tokens, in their original order.
        let result_tokens: Vec<&str> = chain.split(' ').collect();
        assert_eq!(result_tokens.len(), 8);
        // First 3 are tokens 0..3 of the original full chain (0..19 then parent=19).
        let original_full: Vec<String> = (0..20).map(long_id).collect();
        assert_eq!(result_tokens[0], original_full[0]);
        assert_eq!(result_tokens[1], original_full[1]);
        assert_eq!(result_tokens[2], original_full[2]);
        // Last 5 are tokens 15..20.
        assert_eq!(result_tokens[3], original_full[15]);
        assert_eq!(result_tokens[7], original_full[19]);
    }

    #[test]
    fn references_short_chain_not_pruned() {
        // 5 short tokens — stay well under the 500-char limit.
        let chain = build_references_chain(Some("<a@h> <b@h> <c@h> <d@h>"), "<e@h>");
        assert_eq!(chain, "<a@h> <b@h> <c@h> <d@h> <e@h>");
    }

    // --- format_reply_quote ---

    #[test]
    fn reply_quote_single_line_body() {
        let from = Contact::new("Alice", "alice@example.com");
        let q = format_reply_quote(&from, "Thu, 7 Apr 2026 at 14:32", "hello world");
        assert_eq!(
            q,
            "On Thu, 7 Apr 2026 at 14:32, Alice <alice@example.com> wrote:\n> hello world"
        );
    }

    #[test]
    fn reply_quote_multi_line_body_each_line_prefixed() {
        let from = Contact::new("Alice", "alice@example.com");
        let body = "first line\nsecond line";
        let q = format_reply_quote(&from, "Thu, 7 Apr 2026", body);
        assert!(q.ends_with("\n> first line\n> second line"));
    }

    #[test]
    fn reply_quote_nested_quotes_preserved() {
        let from = Contact::new("Bob", "bob@example.com");
        let body = "> already quoted\nplain";
        let q = format_reply_quote(&from, "today", body);
        assert!(q.contains("\n> > already quoted"));
        assert!(q.contains("\n> plain"));
    }

    #[test]
    fn reply_quote_empty_body_attribution_only() {
        let from = Contact::new("Alice", "alice@example.com");
        let q = format_reply_quote(&from, "today", "");
        assert_eq!(q, "On today, Alice <alice@example.com> wrote:");
        assert!(!q.contains('\n'));
    }

    #[test]
    fn reply_quote_blank_lines_become_bare_gt() {
        let from = Contact::new("Alice", "alice@example.com");
        let body = "first\n\nsecond";
        let q = format_reply_quote(&from, "today", body);
        // The blank line in the middle should be `>` (no trailing space).
        assert!(q.contains("\n> first\n>\n> second"));
    }

    // --- format_forward_quote ---

    #[test]
    fn forward_quote_separator_and_headers() {
        let from = Contact::new("Alice", "alice@example.com");
        let to = vec![Contact::new("Bob", "bob@example.com")];
        let q = format_forward_quote(&from, "Thu, 7 Apr 2026", "Subj", &to, "body line");
        assert!(q.starts_with("---------- Forwarded message ----------\n"));
        assert!(q.contains("From: Alice <alice@example.com>\n"));
        assert!(q.contains("Date: Thu, 7 Apr 2026\n"));
        assert!(q.contains("Subject: Subj\n"));
        assert!(q.contains("To: Bob <bob@example.com>\n"));
        // Body is NOT quote-prefixed.
        assert!(q.ends_with("\nbody line"));
        assert!(!q.contains("> body line"));
    }

    #[test]
    fn forward_quote_multiple_to_recipients_joined() {
        let from = Contact::new("A", "a@h");
        let to = vec![Contact::new("B", "b@h"), Contact::new("C", "c@h")];
        let q = format_forward_quote(&from, "today", "S", &to, "");
        assert!(q.contains("To: B <b@h>, C <c@h>\n"));
    }

    // --- reply_all_recipients ---

    #[test]
    fn reply_all_normal_excludes_user() {
        let from = Contact::new("Alice", "alice@example.com");
        let to = vec![
            Contact::new("Me", "me@example.com"),
            Contact::new("Carol", "carol@example.com"),
        ];
        let cc = vec![Contact::new("Dave", "dave@example.com")];
        let (reply_to, reply_cc) = reply_all_recipients(&from, &to, &cc, "me@example.com");
        // To = [original.from]
        assert_eq!(reply_to.len(), 1);
        assert_eq!(reply_to[0].address, "alice@example.com");
        // Cc = (To ∪ Cc) \ user, in order: Carol, Dave (Me dropped).
        assert_eq!(reply_cc.len(), 2);
        assert_eq!(reply_cc[0].address, "carol@example.com");
        assert_eq!(reply_cc[1].address, "dave@example.com");
    }

    #[test]
    fn reply_all_dedups_case_insensitively() {
        let from = Contact::new("Alice", "alice@example.com");
        // Carol appears once in To and once in Cc, with different casing.
        let to = vec![Contact::new("Carol", "Carol@Example.COM")];
        let cc = vec![Contact::new("Carol", "carol@example.com")];
        let (_, reply_cc) = reply_all_recipients(&from, &to, &cc, "me@example.com");
        assert_eq!(
            reply_cc.len(),
            1,
            "case-insensitive dedup must collapse Carol"
        );
    }

    #[test]
    fn reply_all_reply_to_self_preserves_original_to() {
        // The user sent the original; ReplyAll must NOT reply to themselves.
        let from = Contact::new("Me", "me@example.com");
        let to = vec![
            Contact::new("Alice", "alice@example.com"),
            Contact::new("Bob", "bob@example.com"),
        ];
        let cc = vec![Contact::new("Carol", "carol@example.com")];
        let (reply_to, reply_cc) = reply_all_recipients(&from, &to, &cc, "me@example.com");
        // To = original.to (Me is not in the original To list).
        assert_eq!(reply_to.len(), 2);
        assert_eq!(reply_to[0].address, "alice@example.com");
        assert_eq!(reply_to[1].address, "bob@example.com");
        // Cc = original.cc.
        assert_eq!(reply_cc.len(), 1);
        assert_eq!(reply_cc[0].address, "carol@example.com");
    }

    #[test]
    fn reply_all_reply_to_self_strips_user_from_to_and_cc() {
        // Reply-to-self where the user accidentally added themselves to To/Cc.
        let from = Contact::new("Me", "me@example.com");
        let to = vec![
            Contact::new("Me", "me@example.com"),
            Contact::new("Alice", "alice@example.com"),
        ];
        let cc = vec![Contact::new("Me", "me@example.com")];
        let (reply_to, reply_cc) = reply_all_recipients(&from, &to, &cc, "me@example.com");
        assert_eq!(reply_to.len(), 1);
        assert_eq!(reply_to[0].address, "alice@example.com");
        assert!(reply_cc.is_empty());
    }
}
