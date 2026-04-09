# Inboxly — TODOs

## Priority: Wire bundler traits to Store

**What:** Implement `RuleStore`, `AffinityStore`, `BundleStore` (from `inboxly-bundler`) on `inboxly-store::Store`. Delete the aspirational `inboxly-core::traits` module (Store/Bundler/Extractor traits that were never implemented).

**Why:** The M13 `BundlerEngine` accepts `&dyn RuleStore` etc. but can only receive mock implementations today. The real `Store` has the underlying SQL methods (`bundle_rules.rs`, `sender_affinity.rs`, `bundles.rs`) but doesn't implement the traits. Without this, the bundler can't run against real data.

**How to apply:**
1. In `inboxly-store`, add `impl inboxly_bundler::RuleStore for Store { ... }` delegating to existing `bundle_rules.rs` methods
2. Same for `AffinityStore` (delegating to `sender_affinity.rs`) and `BundleStore` (delegating to `bundles.rs`)
3. Add integration tests in `inboxly-store/tests/` that exercise the full bundler pipeline through real SQLite
4. Delete `inboxly-core/src/traits.rs` and remove from `lib.rs` re-exports
5. This introduces a circular dependency risk: `inboxly-store` would need to depend on `inboxly-bundler` for the trait definitions. **Solution**: Move the trait definitions to `inboxly-core` (replacing the current aspirational traits with the concrete bundler traits), or use a separate `inboxly-traits` crate.

**Blocked by:** Nothing
**Blocks:** M15 (UI wiring needs Store to provide bundler functionality)

---

# M37 / Post-M36 polish items

Carried over from the M36 + M36.1 merge. None of these block M37 (Full
Attachment Support) — they're cleanup items surfaced during the M36
subagent-driven-development run that were deliberately deferred to
avoid mid-milestone scope creep. Address them in M37 or a dedicated
M36.2 polish milestone, whichever lands first.

## M37-1: Byte-offset streaming for forward attachment extraction

**What:** Replace the current decode-to-memory `get_body_raw()` path in `inboxly-store/src/forward_attachments.rs` with a true streaming extractor that reads raw encoded bytes from the source `.eml` by part offset + boundary range, decoding only at send time.

**Why:** M36 Phase 9 shipped the simpler decode-to-memory approach because `mailparse 0.15` does not expose per-part byte offsets cleanly — implementing offset tracking from scratch would have been 200+ LOC for a speculative optimization. Current peak memory per forward extraction is ONE attachment's decoded size (~20 MB for a large PDF). Acceptable for everyday use but scales poorly for multi-attachment forwards of very large emails. Gemini G2 + eng review B2 both flagged this as the "right" long-term shape.

**How to apply:**
1. Investigate whether `mailparse` 0.16+ exposes `ParsedMail::raw_bytes` or similar byte-range accessors (check their CHANGELOG between 0.15 and current)
2. If yes: use the native API and call `std::io::copy` with a `SeekFrom::Start(offset) + take(len)` reader to stream bytes to the per-draft destination without a full `Vec` allocation
3. If no: implement manual RFC 5322 boundary tracking in a sibling extractor function. Parse headers with `mailparse::parse_headers` to get the content-type + boundary, then walk the source file byte-by-byte tracking boundary positions
4. Each attachment's encoded bytes get copied directly (still base64/quoted-printable — don't decode) to `<draft_dir>/<on-disk-name>.part` alongside a small `<name>.part.meta.json` sidecar recording `encoding + content_type`
5. At send time, `inboxly-imap::smtp::message_builder` reads the `.part` file, decodes on-the-fly via `base64::read::DecoderReader`, and feeds into lettre's `Attachment` builder
6. 5 new tests: 20 MB fixture attachment (assert memory bound via tracing subscriber allocator probe OR functional size assertion), nested multipart fixture, quoted-printable attachment, malformed boundary fallback, source-file-unlinked-mid-extract error path
7. Update the `TODO(post-M36)` comment in `forward_attachments.rs` module header once this lands

**Blocked by:** Nothing
**Blocks:** Nothing — M36's decode-to-memory path works for realistic email sizes

## M37-2: Compose signal split refactor

**What:** Move `compose: ComposeState` out of the `Inboxly` struct into its own top-level Dioxus context via `use_context_provider` / `use_context::<Signal<ComposeState>>()`. Split the compose update handlers out of `Inboxly::update` into a separate `ComposeState::update` function (or mirror pattern). Update the ~260 existing `self.compose.*` / `app.compose.*` references in `inboxly-ui/src/app.rs` (195), `components/app.rs` (43), `components/compose_view.rs` (21), and `components/toolbar.rs` (3) to read/write the new Signal.

**Why:** M36 Phase 11 deferred this from the original plan because it was a ~300-LOC refactor touching every compose dispatch site + every M35 state-machine test (~50 touches), and the risk of subtle regressions mid-milestone outweighed the perf win. Eng review D1 is still the right shape: without the split, typing in the inline compose panel re-runs `ContentArea` → `ThreadDetailView` → `sanitize_html` on every keystroke because compose state lives on `Inboxly`, which drives `ContentArea` reactively. With the split, compose typing only invalidates the compose subtree, and `sanitize_html` stays bounded at "once per thread load" instead of "once per keystroke". Matches the M34 body signal pattern already in place.

**How to apply:**
1. Create a new top-level `Signal<ComposeState>` via `use_context_provider` in `components/app.rs::App`, alongside the existing `Signal<Inboxly>` and `Signal<Option<Arc<LoadedThread>>>`
2. Delete `pub compose: ComposeState` from `Inboxly` struct
3. Route all `Compose*` message handlers via a new `ComposeState::update(&mut self, msg: Message, accounts: &[AccountConfig], store: &Option<Arc<Store>>)` method OR add a parallel match in the update pipeline that takes `&mut ComposeState` as a second parameter. **Prefer the method-on-ComposeState shape** — it keeps the state-machine tests readable
4. Update all three compose bridges in `components/app.rs` (auto-save, explicit save, picker, send, Navigate guard, reply-prefill) to read/write the new signal instead of `app_state.read().compose`
5. Update the `Inboxly::with_accounts` / `with_store` / `default` test constructors — remove the `compose` field init
6. Update all 50+ state-machine tests that do `app.compose.foo = ...` or `app.update(Message::Compose*)` — they'll need to operate on a separate `ComposeState` fixture
7. Benchmark before/after: record frame time during typing in an inline reply on a 50-message thread. Expect 5-10x speedup on the keystroke hot path
8. Remove the `TODO(post-M36)` comment in `content_area.rs::ContentArea` that references eng review D1

**Blocked by:** Nothing
**Blocks:** Any future perf work on the inline compose flow

## M37-3: IMAP AppendDraft replay handler body

**What:** Implement the real body of `OfflineAction::AppendDraft` in `inboxly-imap/src/offline_replay.rs::replay_single_action`. Currently it's a `tracing::warn!("AppendDraft replay not yet implemented (post-M36 scope)")` stub added in M36 Phase 5 alongside the variant definition.

**Why:** M36 Phase 5's explicit save bridge enqueues `AppendDraft` on every explicit save, but the replay side is a no-op. This means drafts saved locally via the explicit Save Draft button never appear on the IMAP server's `[Gmail]/Drafts` / `Drafts` folder, so the user can't resume a draft from another client (web Gmail, mobile) between sessions. Pair with the existing `AppendSent` implementation shipped in Phase 4 — the patterns are nearly identical; only the target folder and filename lookup differ.

**How to apply:**
1. Mirror `OfflineAction::AppendSent` arm at `offline_replay.rs:282`. Copy the structure verbatim, swap `StandardFolder::Sent` for `StandardFolder::Drafts` and `well_known.sent` for `well_known.drafts`
2. Use `maildir.find_message_id(StandardFolder::Drafts, draft_message_id)` to locate the local Maildir `.Drafts/` file
3. Read the `.eml` bytes, resolve server Drafts folder via `well_known.drafts.as_deref().unwrap_or("Drafts")` (Gmail: `[Gmail]/Drafts`, Outlook: `Drafts`, Fastmail: `Drafts`)
4. `session.select(drafts_folder).await` + `session.append(drafts_folder, Some(r"(\Draft)"), None, bytes.as_slice()).await` — note the `\Draft` flag instead of `\Seen`
5. On success, remove queue entry (automatic via the enclosing `dequeue_offline_action` call in `replay_offline_queue`)
6. Verify the M36.1 Maildir write path actually lands bytes in `.Drafts/` on explicit save — trace through `inboxly-ui/src/components/app.rs::write_local_maildir_drafts` (Phase 5 helper) to confirm the file path matches what `find_message_id` scans
7. 3 new tests in `offline_replay.rs::tests::appenddraft_*`: message-id found → reads bytes, message-id not found → Ok+skip, corrupt bytes → error propagated (matching the Phase 4 AppendSent test structure)

**Blocked by:** Nothing
**Blocks:** Cross-device draft continuity (user resumes web Gmail draft from desktop or vice versa)

## M37-4: Expanded quoted-original preview in compose

**What:** Replace the M36 Phase 12 placeholder italic label (`"Quoted original below the cursor"`) in `inboxly-ui/src/components/compose_view.rs` with a real collapsed summary showing the original sender name + address + date + subject. Add an expand toggle so the user can see the full quoted body inline without scrolling into the textarea.

**Why:** Phase 12 shipped the placeholder because extracting the original's sender/date/subject requires new dedicated fields on `ComposeState` populated at Phase 7's `compose_state_from_original` prefill time — the original's data is currently embedded in `body_markdown` which is fragile to parse. A future polish was punted to avoid a mid-milestone ComposeState schema change. Having a proper summary header is real UX value: the user loses context of who they're replying to once they start typing above the quote block.

**How to apply:**
1. Add three fields to `ComposeState` (in `inboxly-ui/src/state/compose_state.rs`):
   ```rust
   pub original_sender_display: Option<String>,     // "Alice <alice@example.com>"
   pub original_subject_display: Option<String>,    // raw original subject, unstripped
   pub original_date_display: Option<String>,       // pre-formatted "Thu, 7 Apr 2026 at 14:32"
   ```
2. Update `ComposeState::new()` to initialize all three to `None`
3. In `inboxly-ui/src/components/app.rs::compose_state_from_original`, populate these fields from the `LoadedEmail` during prefill — reuse the `format_email_row_date` helper from Phase 7 for the date
4. Update `compose_state_to_draft_email` to NOT propagate these to `DraftEmail` (they're UI-only — the DraftEmail already has enough data via `in_reply_to` + `references`)
5. In `compose_view.rs`, replace the placeholder div with a real `.compose-quoted-original` block rendering `"{original_sender_display} — {original_date_display} — {original_subject_display}"` when `is_reply_or_forward && compose.original_sender_display.is_some()`
6. Add an expand toggle (a chevron icon + `use_signal::<bool>` local state) that when expanded shows the first ~10 lines of the quoted body extracted from `body_markdown` via simple line splitting
7. Add CSS to Section 18 for the new classes: `.compose-quoted-original`, `.compose-quoted-original-expanded`, `.compose-quoted-original-toggle`
8. 2 new tests: prefill populates the three fields correctly for Reply/ReplyAll/Forward, New mode leaves all three as None

**Blocked by:** Nothing
**Blocks:** Nothing — current placeholder is functional

## M37-5: Auto-save bridge no-accounts guard

**What:** Gate the Phase 10 auto-save bridge spawn in `inboxly-ui/src/components/app.rs` on `!accounts.is_empty()` so it does not fire a `tracing::warn!` every 30 seconds when the binary launches with no configured accounts.

**Why:** Spotted during Alan's M36 Tier 1/Tier 2 smoke test of the running binary on 2026-04-08. With no config file, the binary launches cleanly, but the auto-save bridge's 30s timer keeps firing and hitting the `"compose auto-save: no FROM account or no draft_id, skipping save"` warn branch every 30 seconds. It's not a bug — it's the fail-soft path — but it's noisy log pollution during initial setup before the user has added an account. A single account-empty check at the top of the bridge would eliminate all the noise.

**How to apply:**
1. In `inboxly-ui/src/components/app.rs`, find the auto-save bridge's outer `use_effect` block (Phase 10 bridge, search for `"compose auto-save"`)
2. Add a short-circuit at the top of the spawned task's peek block: `if snapshot.accounts.is_empty() { return; }` placed BEFORE the existing FROM-account resolution. This way the timer still fires (so the bridge stays reactive to future account additions via Settings), but the warn is skipped entirely when no accounts exist
3. Alternatively, gate the `use_effect` dependency memo to include `!accounts.is_empty()` — more conservative, but prevents the timer from even spawning. **Prefer the in-task early return** because it's simpler and the wake cost is negligible
4. Add one test that a `ComposeAutoSaveTick` is NOT dispatched when `accounts.is_empty()` (mock the bridge body as a pure function if needed)
5. Verify via manual dogfooding: run `./target/debug/inboxly` for 5+ minutes with no config, grep the stderr for `"compose auto-save"` — should be zero hits

**Blocked by:** Nothing
**Blocks:** Nothing — purely cosmetic log cleanliness
