# Lettre 0.11 API verification for M35b

**Date:** 2026-04-07
**Lettre version:** 0.11.21 (latest 0.11.x at time of writing; `version = "0.11"` in `Cargo.toml` resolves to `^0.11`, which matches `>=0.11.0, <0.12.0`)
**Cached crate path:** Not in `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/` yet — workspace dependencies declared in the root `[workspace.dependencies]` table are not downloaded by `cargo fetch` until at least one workspace member references them via `lettre.workspace = true`. Phase 0 deliberately does NOT add per-crate uses, so the cached source materializes only after Phase 1 wires `lettre.workspace = true` into the crate that owns SMTP. Until then, the source for Phase 0 verification was inspected from a temporary download of the published `.crate` tarball. Verbatim line numbers below are stable for 0.11.21 and should be re-checked if `cargo update -p lettre` later resolves to a higher 0.11.x.
**Verified by:** Phase 0 of M35b (`feature/m35b-smtp-compose`)

## Purpose

Before M35b writes any SMTP code, verify two API unknowns in lettre 0.11:
1. Does `Mechanism::Xoauth2` exist as a built-in?
2. How do we ensure Bcc recipients go into the SMTP envelope but NOT the rendered headers?

## XOAUTH2 mechanism path

**Outcome:** Built-in, NO feature gate. `lettre::transport::smtp::authentication::Mechanism::Xoauth2` is an unconditional enum variant. Lettre 0.11 also constructs the full SASL XOAUTH2 client response string internally — consumers do not need to base64-encode `user=…\x01auth=Bearer …\x01\x01` themselves.

**Evidence:**
- File: `src/transport/smtp/authentication.rs`
- Lines 47-61 — enum declaration. No `#[cfg(...)]` attribute on the type or the `Xoauth2` variant:
  ```rust
  #[derive(PartialEq, Eq, Copy, Clone, Hash, Debug)]
  #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
  pub enum Mechanism {
      Plain,
      Login,
      /// Non-standard XOAUTH2 mechanism, defined in
      /// [xoauth2-protocol](https://developers.google.com/gmail/imap/xoauth2-protocol)
      Xoauth2,
  }
  ```
- Lines 117-123 — internal SASL string construction (matches RFC 7628 / Google's XOAUTH2 spec verbatim):
  ```rust
  Mechanism::Xoauth2 => match challenge {
      Some(_) => Err(error::client("This mechanism does not expect a challenge")),
      None => Ok(format!(
          "user={}\x01auth=Bearer {}\x01\x01",
          credentials.authentication_identity, credentials.secret
      )),
  },
  ```
  This is byte-for-byte the same string `inboxly_imap::auth::xoauth2::build_xoauth2_string()` produces (`inboxly-imap/src/auth/xoauth2.rs:27`). Lettre then base64-wraps it via `Mechanism::supports_initial_response() == true` (line 77) before transmission.
- Line 10 — DEFAULT_MECHANISMS list:
  ```rust
  pub const DEFAULT_MECHANISMS: &[Mechanism] = &[Mechanism::Plain, Mechanism::Login];
  ```
  IMPORTANT: `Xoauth2` is NOT in the default list. Phase 3 MUST explicitly call `.authentication(vec![Mechanism::Xoauth2])` on the `AsyncSmtpTransport::<Tokio1Executor>::relay(...).unwrap().authentication(...)` builder, otherwise lettre will negotiate PLAIN/LOGIN even if the server advertises XOAUTH2.
- Line 189-190 — there is a `test_xoauth2()` unit test in lettre's own suite, so the mechanism is exercised in lettre CI.
- Other references found by grepping `[Xx][Oo][Aa][Uu][Tt][Hh]2` across the entire crate:
  - `src/transport/smtp/mod.rs:11` — module-level doc comment listing supported mechanisms.
  - `src/transport/smtp/extension.rs:163-164` — server EHLO parser recognizes the `XOAUTH2` token and inserts `Extension::Authentication(Mechanism::Xoauth2)` into the negotiated feature set.
  - `src/transport/smtp/extension.rs:393, 402` — extension-parser unit tests cover XOAUTH2.
  - `CHANGELOG.md:653` — initial XOAUTH2 support added in lettre's history (long predates 0.11).

**Phase 3 implementation guidance:**

Take this path. Use `lettre::transport::smtp::authentication::{Credentials, Mechanism}` directly. Do NOT call `inboxly_imap::auth::xoauth2::build_xoauth2_string()` for SMTP — pass the bare access token as the `password` arg:

```rust
use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::transport::smtp::AsyncSmtpTransport;
use lettre::Tokio1Executor;

let creds = Credentials::new(email.to_string(), access_token.to_string());
let transport: AsyncSmtpTransport<Tokio1Executor> =
    AsyncSmtpTransport::<Tokio1Executor>::relay(smtp_host)?
        .port(smtp_port)
        .credentials(creds)
        .authentication(vec![Mechanism::Xoauth2]) // CRITICAL: not in DEFAULT_MECHANISMS
        .build();
```

Keep `inboxly_imap::auth::xoauth2::build_xoauth2_string()` exactly as-is — it is still used by the IMAP path (`async-imap` doesn't have a built-in SASL helper), so deleting it would break IMAP. The two paths intentionally diverge: lettre owns its SMTP SASL string, our `auth::xoauth2` module owns the IMAP SASL string, and both produce the same RFC 7628 bytes from the same `(email, access_token)` inputs.

## Bcc envelope-only API

**Outcome:** Bcc IS hidden from the rendered headers by default. `MessageBuilder::bcc(mbox)` adds the recipient to the `Bcc:` header, the envelope is then constructed from headers (Bcc included in the recipient list), and finally the `Bcc:` header is dropped from the rendered message. Privacy is enforced. There is also an explicit opt-out (`MessageBuilder::keep_bcc()`) for the Sent-folder-append case that needs the header preserved.

**Evidence:**

Note: the message builder lives in `src/message/mod.rs`, NOT in a separate `builder.rs` file (no such file exists in 0.11.21).

- `MessageBuilder` struct definition: `src/message/mod.rs:232-238`
  ```rust
  #[derive(Debug, Clone)]
  pub struct MessageBuilder {
      headers: Headers,
      envelope: Option<Envelope>,
      drop_bcc: bool,
  }
  ```
  The `drop_bcc` field is the privacy switch.

- `MessageBuilder::new()`: `src/message/mod.rs:240-248` — sets `drop_bcc: true` as the default.

- `MessageBuilder::bcc()`: `src/message/mod.rs:306-311`
  ```rust
  /// Set or add mailbox to `Bcc` header
  ///
  /// Shortcut for `self.mailbox(header::Bcc(mbox))`.
  pub fn bcc(self, mbox: Mailbox) -> Self {
      self.mailbox(header::Bcc(mbox.into()))
  }
  ```
  At this stage the recipient is just a header entry — no envelope split has happened yet.

- `MessageBuilder::keep_bcc()`: `src/message/mod.rs:395-407` — the documented opt-out:
  ```rust
  /// Keep the `Bcc` header
  ///
  /// By default, the `Bcc` header is removed from the email after
  /// using it to generate the message envelope. In some cases though,
  /// like when saving the email as an `.eml`, or sending through
  /// some transports (like the Gmail API) that don't take a separate
  /// envelope value, it becomes necessary to keep the `Bcc` header.
  ///
  /// Calling this method overrides the default behavior.
  pub fn keep_bcc(mut self) -> Self {
      self.drop_bcc = false;
      self
  }
  ```

- `MessageBuilder::build()` — the actual privacy enforcement: `src/message/mod.rs:412-451`
  ```rust
  fn build(self, body: MessageBody) -> Result<Message, EmailError> {
      // ... date / from / sender checks ...

      let envelope = match res.envelope {
          Some(e) => e,
          None => Envelope::try_from(&res.headers)?, // <-- envelope built from headers (line 438)
      };

      if res.drop_bcc {
          // Remove `Bcc` headers now the envelope is set
          res.headers.remove::<header::Bcc>(); // <-- header stripped (line 443)
      }

      Ok(Message {
          headers: res.headers, // <-- Message stores Bcc-stripped headers
          body,
          envelope,             // <-- but envelope still has Bcc recipients
      })
  }
  ```
  The order is the load-bearing detail: envelope is materialized from headers BEFORE the Bcc header is removed.

- `Envelope::try_from(&Headers)`: `src/address/envelope.rs:165-203` — confirms the envelope's `to` (forward path) field aggregates To + Cc + Bcc:
  ```rust
  let mut to = vec![];
  add_addresses_from_mailboxes(&mut to, headers.get::<header::To>().map(|h| h.0));
  add_addresses_from_mailboxes(&mut to, headers.get::<header::Cc>().map(|h| h.0));
  add_addresses_from_mailboxes(&mut to, headers.get::<header::Bcc>().map(|h| h.0)); // line 199
  Self::new(from, to)
  ```
  All three header types feed the SMTP `RCPT TO` list — this is what makes Bcc actually deliver while still being absent from the rendered message.

- `Message::format()` (the rendered output writer): `src/message/mod.rs:605-609`
  ```rust
  impl EmailFormat for Message {
      fn format(&self, out: &mut Vec<u8>) {
          write!(out, "{}", self.headers)
              .expect("A Write implementation panicked while formatting headers");
          // ... body ...
  ```
  `format()` writes `self.headers`, which is the Bcc-stripped Headers struct from line 447. The `Bcc:` header simply does not exist in the bytes that go on the wire to MTAs or appended to a `.eml`.

- `Message::envelope()` getter: `src/message/mod.rs:518-520` — exposes the envelope (which still has Bcc) for transport layers that need it.

- `header::Bcc` type: `src/message/header/mailbox.rs:166-175` — generated by the `mailboxes_header!` macro. Public path: `lettre::message::header::Bcc`.

**The mechanism (verbatim explanation):**

`MessageBuilder` accumulates Bcc recipients in its `headers: Headers` field via `bcc()` (or `mailbox(header::Bcc(...))`). On `build()`, lettre snapshots the recipient list into a separate `Envelope` struct using `Envelope::try_from(&headers)` — that snapshot includes To, Cc, AND Bcc addresses in a single forward-path vector. Immediately after the snapshot, if `drop_bcc` is true (the default), `headers.remove::<header::Bcc>()` strips the Bcc header from the headers store. The resulting `Message { headers, body, envelope }` has Bcc recipients in `envelope.to()` (used by SMTP `RCPT TO`) but completely absent from `headers`, so `Message::formatted()` and any DKIM signing operate on Bcc-free bytes.

**Phase 3 implementation guidance:**

- **SMTP transmission builder (the wire send):** Use the default `MessageBuilder::bcc()` and DO NOT call `keep_bcc()`. lettre's `AsyncTransport::send(...)` will pull the envelope (with Bcc in the forward path) and the rendered bytes (with Bcc removed) automatically. No special handling required.

- **Sent-folder APPEND builder (Gemini G1 — the IMAP `APPEND` body that the user later sees in their Sent folder):** Build a *second* `Message` from the same `DraftEmail`, this time chaining `.keep_bcc()` BEFORE `.body(...)` / `.multipart(...)`. This produces a Message where `formatted()` includes the `Bcc:` header so the user can see who they Bcc'd in their own Sent view. The two builders share all other field values — only the `keep_bcc()` call differs. Both should derive from the same `DraftEmail` to guarantee they don't drift.

  Sketch:
  ```rust
  fn build_smtp_message(draft: &DraftEmail) -> Result<lettre::Message, EmailError> {
      let mut builder = lettre::Message::builder()
          .from(draft.from.parse()?)
          .subject(&draft.subject);
      for to in &draft.to   { builder = builder.to(to.parse()?); }
      for cc in &draft.cc   { builder = builder.cc(cc.parse()?); }
      for bcc in &draft.bcc { builder = builder.bcc(bcc.parse()?); }
      // drop_bcc defaults to true → wire bytes have no Bcc header
      builder.body(/* body */)
  }

  fn build_sent_folder_message(draft: &DraftEmail) -> Result<lettre::Message, EmailError> {
      let mut builder = lettre::Message::builder()
          .from(draft.from.parse()?)
          .subject(&draft.subject)
          .keep_bcc(); // <-- ONLY difference: preserve Bcc in rendered headers
      for to in &draft.to   { builder = builder.to(to.parse()?); }
      for cc in &draft.cc   { builder = builder.cc(cc.parse()?); }
      for bcc in &draft.bcc { builder = builder.bcc(bcc.parse()?); }
      builder.body(/* body */)
  }
  ```

  Order matters: `keep_bcc()` only flips the boolean — it has no positional dependency on `bcc()` calls — but as a stylistic rule, place it immediately after `Message::builder()` for visibility. Manually constructing the `Bcc:` header via `lettre::message::header::Bcc` is NOT needed; `bcc()` plus `keep_bcc()` does the right thing.

- **Tests Phase 3 should write to lock this in:**
  1. Build a draft with at least one To, one Cc, and one Bcc. Build the SMTP message via the first helper. Assert: `message.envelope().to().len() == 3`, AND `String::from_utf8(message.formatted()).unwrap()` does NOT contain `Bcc:` (case-insensitive). This guards against an accidental future call to `keep_bcc()` on the SMTP builder.
  2. Build the same draft via the Sent-folder helper. Assert: rendered bytes DO contain `Bcc:` and the Bcc address.
  3. Build a draft with Bcc but no To/Cc. Assert the SMTP envelope still has the Bcc recipient in its forward path. (Edge case — RFC 5322 doesn't require a To header, and lettre should handle it; if it errors, this test will surface it.)

## Open questions / risks

- **Lockfile state.** Cargo does not pull `lettre` into `Cargo.lock` from `[workspace.dependencies]` alone. Phase 1 will need to add `lettre.workspace = true` to whichever crate owns SMTP transport (likely a new `inboxly-smtp` crate per the M35 plan, or `inboxly-imap` if reused). The first `cargo build` after that change will resolve `lettre = "0.11"` against `^0.11`, which today picks 0.11.21. Phase 1's commit will be the one that adds the actual `lettre` block to `Cargo.lock`.

- **Version drift between Phase 0 verification and Phase 3 execution.** All line numbers cited above are from 0.11.21. If a 0.11.22+ ships before Phase 3 lands, the constants might shift. Phase 3's first action should be to confirm the cached crate version with `cargo metadata --format-version 1 | jq '.packages[] | select(.name=="lettre") | .version'` and re-grep for `drop_bcc` and `Mechanism::Xoauth2` if it differs from 0.11.21. The *behaviour* contracts (XOAUTH2 enum exists, Bcc is dropped by default with `keep_bcc()` opt-out) have been stable across the entire 0.11 line per the changelog; only line numbers might shift.

- **`DEFAULT_MECHANISMS` does not include XOAUTH2.** This is the easiest mistake for Phase 3 to make. If you forget the `.authentication(vec![Mechanism::Xoauth2])` call, lettre will try PLAIN against Gmail and Gmail will reject with `534-5.7.9 Application-specific password required`. Add a unit test that asserts the configured transport's mechanism list contains `Xoauth2` to lock this in. (Lettre exposes the configured mechanisms via the builder's internal state — easiest way to assert is a small wrapper that records the mechanism vec on construction.)

- **`AsyncTransport` trait method shape (not directly asked but worth recording while we have the source open).** SMTP transports implement `lettre::AsyncTransport` from `src/transport/mod.rs`. The two methods Phase 3 will use are `send(message: Message)` (high-level — handles envelope + format internally) and `send_raw(envelope: &Envelope, body: &[u8])` (low-level — for the offline replay path where the body is already serialized to disk). Both return `Result<Self::Ok, Self::Error>`. The `Ok` type for `AsyncSmtpTransport<Tokio1Executor>` is `lettre::transport::smtp::response::Response`, which exposes `.code()` and `.message()` for retry-decision logic.

- **Feature flag confirmation against the workspace `Cargo.toml` diff.** The features the Phase 0 commit declares are `tokio1-rustls-tls` (async transport over tokio + rustls), `smtp-transport` (the SMTP transport itself — without this, only the `Message` builder is available), `builder` (the `MessageBuilder` type — see the `#[cfg(feature = "builder")]` gate on `impl TryFrom<&Headers> for Envelope` at `src/address/envelope.rs:165`), and `tracing` (structured logs from the transport layer). All four are required for the M35b SMTP path; none of them gate XOAUTH2 (which is unconditional) or `keep_bcc()` (which is gated only on `builder`). The `default-features = false` opt-out drops `native-tls`, `pool`, `hostname`, `mime03`, `webpki-roots`, `serde`, `rustls-platform-verifier`, and `dkim` — none of which Phase 3 needs.

- **No SMTP code yet — Phase 0 ends here.** Phase 1 wires `lettre.workspace = true` into the per-crate Cargo.toml and adds `pulldown-cmark` and `rfd`. Phase 3 implements the actual transport using this notes file as the source of truth.
