# Keyring 3.x + OAuth2 persistence API verification for M36

**Date:** 2026-04-08
**Keyring crate version:** 3.6.3 (latest stable on crates.io as of `2025-07-27`; `4.0.0-rc.3` exists as a pre-release but is gated behind explicit opt-in and is NOT what `keyring = "3"` resolves to)
**Verified by:** Phase 0 of M36 (`feature/m36-reply-forward`)
**Cached crate path:** Not yet — like the lettre verification before it, Phase 0 deliberately does NOT add `keyring` to any per-crate `Cargo.toml`, so `cargo fetch` will not materialize the source until Phase 1 wires it into `inboxly-secrets` (or whichever crate ends up owning the secrets backend). All crate-feature facts below come from the crates.io API metadata for 3.6.3.

## Purpose

Before M36 Phase 1 writes any keyring code, verify three API unknowns:

1. Which `keyring = "3"` feature flag set yields a working Linux Secret Service backend on this dev machine, given that the host runs KDE Plasma 6 with KWallet (not GNOME with gnome-keyring).
2. Does `inboxly-imap`'s existing `oauth2::authorize` already return a token shape (`OAuth2Token { refresh_token: Option<String>, ... }`) that can be persisted as-is, or does Phase 2 need a wrapper?
3. What `service` / `user` naming should the secrets store use so a single `secret-tool search service inboxly` lists every credential and so multi-account setups don't collide?

## Live keyring round-trip

**SKIPPED — explained in Deferred Verification at the end.** The user is at work and a misconfigured `keyring::Entry::new(...).set_password(...)` round-trip can stall behind a KWallet PAM unlock dialog if the wallet isn't already unlocked in the current session, or behind a `org.freedesktop.portal.Secret` consent prompt if `ksecretd` brokers the call. Phase 0 verifies presence + reasoning only; the round-trip happens organically in Phase 1's mock-backed unit tests and is then exercised end-to-end by Phase 14 dogfooding under Alan's eyes.

## Feature flag choice — `linux-native-sync-persistent`

**Outcome:** Use `keyring = { version = "3", features = ["linux-native-sync-persistent"] }`. Do NOT enable `default-features = false` blindly without re-adding this feature, and do NOT enable `apple-native` / `windows-native` since this is a Linux-only project.

**Why this feature, not the others:**

The crates.io feature table for 3.6.3 (verbatim from the `versions/3.6.3` API endpoint) is:

| Feature | Brings in |
|---|---|
| `apple-native` | `security-framework` (macOS Keychain — irrelevant) |
| `async-io` | `zbus` async runtime via `async-io` |
| `async-secret-service` | `secret-service` + `zbus` (async DBus path) |
| `crypto-openssl` | OpenSSL crypto for the Secret Service AES session |
| `crypto-rust` | Pure-Rust crypto for the Secret Service AES session |
| `linux-native` | `linux-keyutils` (in-kernel session keyring — **NOT persistent across logout**) |
| `linux-native-async-persistent` | `linux-native` + `async-secret-service` (kernel keyring primary, async libsecret fallback for persistence) |
| `linux-native-sync-persistent` | `linux-native` + `sync-secret-service` (kernel keyring primary, sync libsecret fallback) |
| `sync-secret-service` | `dbus-secret-service` (sync DBus path; the modern recommended sync binding) |
| `tokio` | `zbus`'s tokio runtime adapter |
| `vendored` | Statically link `dbus-secret-service` and `openssl` |
| `windows-native` | `windows-sys` (Windows Credential Manager — irrelevant) |

The crucial nuance the plan glossed over: **the `linux-native-*` names do NOT mean "libsecret only".** They mean *kernel keyutils as the primary store, with the Secret Service as the persistent fallback layered on top*. The kernel keyutils path is process-session-scoped and disappears on logout — useless for refresh tokens. The `*-persistent` suffix flips on the libsecret fallback so cold-start reads after a reboot still find the entry. This is the modern recommended layout per the keyring crate's own README and its 3.x architecture changes.

Why `linux-native-sync-persistent` over the alternatives:

- **`sync-secret-service` alone** (the option the plan calls "the legacy direct libsecret binding"). This actually works fine — it's just `dbus-secret-service` with no kernel-keyutils layer. The reason to prefer the `linux-native-*` variant is that it avoids one DBus round-trip for the common case (an already-cached entry in the kernel session keyring) without giving up persistence. For Inboxly's read pattern (one keyring lookup per account at startup, plus a refresh write whenever the OAuth2 access token rolls over) the difference is microseconds — but it's free, so take it. There's no downside.
- **`linux-native-async-persistent`.** Inboxly's runtime is tokio-based, so on the surface "async" sounds correct. Don't fall for it. Keyring calls happen at extremely low cadence (once per account at startup, occasionally on token refresh) and they are blocking-by-nature DBus round-trips wrapped in `spawn_blocking` either way. Using `async-secret-service` pulls in `zbus` (with its own tokio/async-io split, requiring us to also pick `tokio` or `async-io` as a sub-feature) and a bigger transitive tree (`zbus`, `zvariant`, `serde`, `enumflags2`, etc.) for zero practical benefit. The sync path uses `dbus-secret-service` which is a much smaller transitive footprint and `tokio::task::spawn_blocking` handles the blocking wrap cleanly. Phase 1 will use exactly this pattern:
  ```rust
  let entry = keyring::Entry::new("inboxly", "oauth2:user@example.com")?;
  let token = tokio::task::spawn_blocking(move || entry.get_password())
      .await
      .map_err(/* JoinError */)?
      .map_err(/* keyring::Error */)?;
  ```
  Bench-relevant: `spawn_blocking` has ~1µs overhead, the DBus round-trip is ~100µs–1ms, the blocking call frees the worker thread for unrelated tasks, and the call site is not on the IMAP fetch hot path. The async crate is the wrong choice here.
- **`async-secret-service` without `linux-native`.** Same async-tax problem as above, plus loses the kernel keyutils fast path, plus forces a runtime choice via `tokio` or `async-io` sub-feature. Strict regression.
- **No feature flag at all.** `keyring` has no default platform feature in 3.x — you MUST opt in to at least one platform backend or `Entry::new` will return `keyring::Error::PlatformFailure` at runtime. This is a breaking change from 2.x and is documented in the 3.0 changelog. Phase 1 unit tests should catch this, but worth knowing.

**Crypto sub-feature.** `linux-native-sync-persistent` does NOT enable a crypto backend by itself. The `dbus-secret-service` crate it pulls in defaults to `crypto-rust` if neither `crypto-openssl` nor `crypto-rust` is selected explicitly — verified by reading `dbus-secret-service`'s own `Cargo.toml` (its `default = ["crypto-rust"]` line). For Inboxly we'll let the default ride: pure-Rust crypto avoids an OpenSSL system dep. If a future audit needs FIPS-validated crypto we'd flip to `crypto-openssl`, but that's not on the table.

**Final Cargo.toml block (Phase 1 will add this):**
```toml
# Workspace root Cargo.toml
[workspace.dependencies]
keyring = { version = "3", default-features = false, features = ["linux-native-sync-persistent"] }

# inboxly-secrets/Cargo.toml (or wherever the secrets backend lives)
keyring.workspace = true
```
`default-features = false` is harmless (3.6.3 declares no default features in its crates.io metadata) but is the explicit form and protects against a future point release adding a default we don't want.

## System verification — Secret Service backend is present and running

```
$ pacman -Q libsecret kwalletd6 2>&1
libsecret 0.21.7-1.1
error: package 'kwalletd6' was not found
```

The `kwalletd6` package query fails because the Arch package is named `kwallet` (not `kwalletd6`); the binary it ships IS `/usr/bin/kwalletd6`. Confirm:

```
$ pacman -Qs kwallet
local/kwallet 6.24.0-2.1 (kf6)
    Secure and unified container for user passwords
local/kwallet-pam 6.6.3-1.1 (plasma)
    KWallet PAM integration
local/ksshaskpass 6.6.3-1.1 (plasma)
local/kwalletmanager 25.12.3-1.1 (kde-applications kde-utilities)
local/signon-kwallet-extension 25.12.3-1.1 (kde-applications kde-network)
```

`kwallet 6.24.0` is the Plasma 6 / KF6 KWallet daemon — exactly what M36 wants. The plan's reference to "kwalletd6" was correct in spirit, just wrong about the package name. This is a false-alarm gap, not a real one, but Phase 0 should call it out so future reviewers don't waste cycles on it.

```
$ ldconfig -p | grep -E "libsecret|kwallet"
        libsecret-1.so.0 (libc6,x86-64) => /usr/lib/libsecret-1.so.0
        libsecret-1.so (libc6,x86-64) => /usr/lib/libsecret-1.so
```

`libsecret-1.so.0` is present at `/usr/lib/libsecret-1.so.0`. `dbus-secret-service` (the crate `linux-native-sync-persistent` pulls in) does NOT link `libsecret` directly — it speaks the Secret Service DBus protocol over `org.freedesktop.secrets` from scratch. So `libsecret` being present is reassuring but not strictly required by the keyring crate path; what matters is that *some* daemon registers the `org.freedesktop.secrets` DBus name. On this box that daemon is `ksecretd`:

```
$ ps -eo comm | grep -iE "keyring|kwallet|secret"
ksecretd
kwalletd6

$ ls /usr/share/dbus-1/services/ | grep -iE "secret|wallet"
org.freedesktop.impl.portal.desktop.kwallet.service
org.kde.kwalletd6.service
org.kde.kwalletmanager.service
org.kde.secretprompter.service
org.kde.secretservicecompat.service
```

Architecture: `dbus-secret-service` → DBus `org.freedesktop.secrets` → **`ksecretd`** (the Plasma 6 Secret Service bridge, registered via `org.kde.secretservicecompat.service`) → **`kwalletd6`** (the actual encrypted store). `ksecretd` is the translation layer that lets non-KDE apps talk Secret Service to a KWallet backend. There is NO `gnome-keyring-daemon` on this box (`pacman -Q gnome-keyring` returns "package not found" and `systemctl --user status gnome-keyring-daemon.service` returns "Unit not found"), so KWallet is the *only* Secret Service provider — no ambiguity about which daemon serves the request.

**Implication for Phase 1 mocks:** The test crate must mock at the `keyring::Entry` level, not by spinning up a real Secret Service. Hitting the real DBus path during `cargo test` would require an unlocked KWallet, which (a) is not available in CI, and (b) would trip the side-effecting-tests rule from `feedback_side_effecting_tests.md`. Use a trait abstraction over the `set_password` / `get_password` / `delete_password` triple, with a `MemorySecretsStore` for tests and a `KeyringSecretsStore` for production. Phase 1 plan should explicitly call this out.

## `oauth2::authorize` signature walkthrough

**Outcome:** The existing function already returns the exact shape M36 needs. No wrapper, no newtype, no shape change. The only thing Phase 2 has to add is "after `authorize` returns Ok, persist `token.refresh_token` to the keyring under `oauth2:<email>`."

**File:** `inboxly-imap/src/auth/oauth2.rs`

**Function signature** (line 76):
```rust
pub async fn authorize(config: &GmailOAuth2Config) -> Result<OAuth2Token>
```
- Single parameter: `&GmailOAuth2Config` (borrowed config, no ownership transfer required by the call site).
- Returns `crate::error::Result<OAuth2Token>`, which is `Result<OAuth2Token, ImapError>`. `ImapError::OAuth2 { reason: String }` is the error variant for every failure path inside the function (port bind, URL parse, browser open failure ignored as warn-only, listener accept, code parse, CSRF state mismatch, token exchange).
- `async` (relies on tokio for the `tokio::net::TcpListener` and the `oauth2` crate's `request_async`). Phase 2 callers will already be in an async context.

**`OAuth2Token` struct** (lines 38–43):
```rust
pub struct OAuth2Token {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<Instant>,
}
```
- `refresh_token` is `Option<String>` — this matches the OAuth2 spec (the authorization server is not required to return a refresh token, e.g. if the user already granted offline access in a previous flow). Phase 2 must handle the `None` case explicitly: log a warning and persist nothing, rather than persisting an empty string or panicking.
- `access_token` is the bearer token (used for the next IMAP/SMTP XOAUTH2 SASL exchange) — DO NOT persist this, it's short-lived (Gmail issues 1-hour tokens) and is rebuilt from `refresh_token` on every cold start via `oauth2::refresh_token` (line 163).
- `expires_at` is an `Option<Instant>` — `Instant` is process-local and monotonic, so it MUST NOT be persisted (it's meaningless across process restarts). On cold start, treat the cached refresh token as "no expiry info available, run `refresh_token` immediately to get a fresh access token + expiry."

**`GmailOAuth2Config::redirect_port_range`** (lines 21, 32):
- Field: `pub redirect_port_range: (u16, u16)` — inclusive `(start, end)` tuple.
- Default value (line 32): `(8080, 8099)` — set by `GmailOAuth2Config::new(...)`.
- Used by `find_available_port` (lines 56–65), which iterates `start..=end` and `TcpListener::bind`s `(127.0.0.1, port)` until one succeeds. Returns the first free port or `ImapError::OAuth2 { reason: "No available port in range ..." }` if every port in the range is occupied.
- The chosen port is fed into the redirect URL (line 89): `RedirectUrl::new(format!("http://127.0.0.1:{port}/callback"))`. Google's OAuth2 client config for the Inboxly app must whitelist `http://127.0.0.1:8080/callback` through `http://127.0.0.1:8099/callback` — 20 redirect URIs. (This is already the case; we're not changing the port range.)

**The happy path**, line by line through `authorize`:

1. Pick an available local port in `(8080, 8099)` (line 77).
2. Build `oauth2::basic::BasicClient` with the Gmail auth/token endpoints and the loopback redirect URL (lines 95–104).
3. Generate a PKCE challenge + verifier (line 107) — PKCE is mandatory for public clients per RFC 7636 + Google's OAuth2 policy.
4. Build the authorization URL with all configured scopes and the PKCE challenge attached (lines 110–116).
5. Open the URL in the user's default browser via `open::that(...)` (line 122). NOTE: This is an `open::that` call — same crate that triggered the M34 side-effecting-tests incident per `feedback_side_effecting_tests.md`. Phase 1 tests of the new persistence layer must NOT exercise this code path. The persistence-layer tests should call only `OAuth2Token::is_expired()` and the keyring-store mocks; they should never reach `authorize`.
6. Bind a real `tokio::net::TcpListener` on the chosen port (line 127) and `wait_for_callback` on it (line 133). `wait_for_callback` accepts one connection, parses the `code=` and `state=` query params from the request line, verifies CSRF state, and sends a small "Authorization successful!" HTML response.
7. Exchange the auth code for tokens via `client.exchange_code(code).set_pkce_verifier(pkce_verifier).request_async(&http_client)` (lines 139–146).
8. Compute `expires_at = now + token_response.expires_in()` if Gmail returned a duration (line 148). Gmail always does, but the spec allows omission, hence `Option<Instant>`.
9. Construct and return `OAuth2Token { access_token, refresh_token, expires_at }` (lines 150–156). The refresh token is extracted via `token_response.refresh_token().map(|t: &oauth2::RefreshToken| t.secret().clone())` — note the explicit type annotation; this is because `oauth2::TokenResponse::refresh_token` returns `Option<&Self::TokenType::RefreshToken>` and the `oauth2` crate's trait resolver can't infer it without help.
10. Caller (today: `inboxly_imap::auth::authenticate`, lines 65–83 of `mod.rs`) caches the returned `OAuth2Token` in-memory in `OAuth2AuthParams.token_cache: Option<OAuth2Token>` and uses `is_expired()` (60-second skew, line 47–52 of `oauth2.rs`) to decide whether to short-circuit, refresh, or full-reauthorize on the next IMAP login.

**Phase 2 implementation guidance:**

After Phase 1 lands the `SecretsStore` trait, Phase 2 should add a thin wrapper at the `inboxly_imap::auth::authenticate` boundary:

```rust
// Pseudocode — exact crate boundaries TBD by Phase 2 plan
let token = match params.token_cache {
    Some(ref t) if !t.is_expired() => t.clone(),
    Some(ref t) if t.refresh_token.is_some() => {
        oauth2::refresh_token(&config, t.refresh_token.as_ref().unwrap()).await?
    }
    _ => {
        // NEW: try keyring before full reauth
        let stored: Option<String> = secrets.get("oauth2", email).await?;
        match stored {
            Some(refresh) => oauth2::refresh_token(&config, &refresh).await?,
            None => {
                let fresh = oauth2::authorize(&config).await?;
                if let Some(ref rt) = fresh.refresh_token {
                    secrets.set("oauth2", email, rt).await?;
                }
                fresh
            }
        }
    }
};
```
The `authorize` function itself is NOT modified — it remains the "prompt the user, run the loopback flow" code. The persistence wrapper happens in the dispatcher one level up. This keeps `authorize` testable in isolation and matches the existing layering.

The `refresh_token` function (lines 163–213) ALSO returns a possibly-rotated refresh token in its result (line 209: `.or_else(|| Some(refresh_token_str.to_string()))`). Google's behaviour is that refresh tokens are typically NOT rotated, but the spec allows rotation, so Phase 2 should re-persist the (possibly new) refresh token after every successful `refresh_token` call to be safe — overwriting the keyring entry with the same value is cheap and idempotent.

## Keyring naming scheme decision

**Decision:** Single service name `"inboxly"`. Per-credential kind prefixed onto the user field:
- Password / app password auth: `service = "inboxly"`, `user = "password:<email>"`
- OAuth2 refresh token: `service = "inboxly"`, `user = "oauth2:<email>"`

Where `<email>` is the lowercased canonical email address from `inboxly_core::Account.email` (lowercased so `Foo@Bar.com` and `foo@bar.com` collapse to one entry — Gmail treats local-part case as case-insensitive in practice).

**Rationale:**

- **Single service prefix.** A single `secret-tool search service inboxly` (or `keyring list inboxly` if we add a CLI helper) lists every secret Inboxly owns. This is critical for support and uninstall: a user wanting to fully purge Inboxly can run `secret-tool clear service inboxly` once and it removes everything. If we used different service names for password vs OAuth2 (e.g. `inboxly-passwords` and `inboxly-oauth2`), the purge would need two commands and the support story gets messier.
- **Per-email partitioning via the `user` field.** Multi-account Inboxly setups (the M36 plan and onward all assume multi-account is a first-class feature) require that account A's refresh token doesn't overwrite account B's. Putting the email in the `user` field is the natural Secret Service idiom for this — `user` is meant to be the per-secret discriminator within a single service.
- **Kind prefix on the user field (not as a separate attribute).** The `keyring` crate's `Entry::new(service, user)` API only takes two strings. For richer attributes (e.g. `kind = "oauth2"` as a separate Secret Service attribute) we'd need `Entry::new_with_target` or to drop down to `dbus-secret-service` directly. We don't need that complexity — embedding the kind as a `kind:email` user-field prefix is unambiguous (the `:` in email addresses' local parts is technically RFC-allowed but in practice never used; even if it were, `password:` and `oauth2:` are the only two prefixes we'll ever issue, both unambiguous from any real email's local part because no real provider permits a colon followed by an `@` later in the string). It also keeps the `secret-tool` listing trivially scannable:
  ```
  service: inboxly  user: password:alan@example.com
  service: inboxly  user: oauth2:alan@example.com
  service: inboxly  user: oauth2:work-account@gmail.com
  ```
  At a glance, every entry tells you which auth method, which email. No JSON, no extra DBus calls.
- **Why not nested service names (`inboxly/oauth2`).** Some Secret Service backends accept slashes in service names; KWallet specifically does not — KWallet uses the service name as a folder name in its GUI tree and slashes get sanitized to `_` inconsistently. Stick with the flat `"inboxly"` service name and discriminate in `user`.
- **Why not put the email in the service name** (e.g. `service = "inboxly-alan@example.com"`). Would multiply the number of services in `secret-tool search service inboxly` to N-per-account, and `secret-tool` does substring matching on service names so the search would still work but `secret-tool clear service inboxly` would NOT (clear is exact-match for service). Worse uninstall story.

**Rust constants Phase 1 should define** (preview, not binding):
```rust
pub const KEYRING_SERVICE: &str = "inboxly";

pub fn user_field(kind: SecretKind, email: &str) -> String {
    let prefix = match kind {
        SecretKind::Password => "password",
        SecretKind::OAuth2Refresh => "oauth2",
    };
    format!("{prefix}:{}", email.to_ascii_lowercase())
}
```
The lowercasing is the load-bearing detail: tests must lock it in (e.g. round-trip a credential with mixed-case email and read it back with lower-case email).

## Deferred verification

The following live checks were intentionally NOT performed in Phase 0 because they would trigger a KWallet PAM unlock dialog or a `ksecretd` consent prompt while the user is at work, blocking indefinitely. They will instead be exercised by Phase 1's mock-backed unit tests (which never touch the real DBus path) and by Phase 14's end-to-end dogfooding (which Alan will run interactively, with the wallet already unlocked).

1. **Real `keyring::Entry::new("inboxly", "oauth2:test@example.com").set_password("...")` round-trip.** This is the full set → get → delete cycle the plan asked for. Must happen with an unlocked KWallet on Alan's interactive session. Phase 1's `KeyringSecretsStore` should ship with an integration test gated behind `#[ignore]` or `#[cfg(feature = "live-keyring-tests")]` so it can be run manually but does not fire during `cargo test --workspace`.
2. **Confirm `linux-native-sync-persistent` actually picks the Secret Service path on this box** (not the kernel keyutils path). The kernel keyutils path is process-session-scoped and would silently lose the entry across logouts — disastrous if it ends up being the chosen tier. The `keyring` crate's docs state that on Linux, with `linux-native-sync-persistent`, the keyutils tier is checked first for cached reads but writes go to BOTH tiers (keyutils + Secret Service) so persistence is guaranteed. Verify this with a logout/login round-trip during Phase 14 dogfooding: write a token, log out of Plasma, log back in, read the token, confirm it's still there.
3. **Confirm `ksecretd` brokers the call to `kwalletd6` correctly without prompting on every read.** The first read of the day after KWallet unlock should NOT prompt; subsequent reads should not prompt either. If `ksecretd` is misconfigured and prompts on every access, Inboxly UX will be unusable. Verify during Phase 14.
4. **Behaviour when KWallet is locked at app startup.** Two reasonable behaviours: (a) `keyring::Entry::get_password` blocks until the user unlocks via the system tray, or (b) it returns `keyring::Error::NoEntry` immediately. We need to know which one happens so the M36 startup path can show a sensible error message ("Unlock KWallet to access stored credentials" vs "No saved credentials, please log in"). This verification belongs in Phase 1's manual test plan, not in `cargo test`.
5. **`secret-tool clear service inboxly` actually removes all Inboxly entries.** Part of the uninstall story; verify during Phase 14 dogfooding by writing two entries, running clear, and confirming both are gone.
6. **`vendored` feature requirements on a clean Arch box.** We're NOT enabling `vendored`, so we depend on the system's `dbus-1` library being present (which it always is on a system that has KWallet installed). If Inboxly is ever distributed as an AppImage or Flatpak, revisit `vendored` then.

## Open questions / risks

- **Plan said `kwalletd6` package; Arch packages it as `kwallet`.** Already noted above. Real risk: zero. Documentation risk: if a future review reads the M36 plan literally and runs `pacman -Q kwalletd6` it'll get a "package not found" and panic. Phase 14 doc-updates should fix the plan to say `kwallet` (Arch) / `kwalletd5` package on Debian-likes / `kf6-kwallet` on Fedora.
- **Keyring 4.0.0-rc.3 exists.** As of `2026-04-08` the keyring crate has a 4.0 release candidate (`4.0.0-rc.3`) but it's pre-release and `keyring = "3"` resolves cleanly to `3.6.3`. Do not rev to `keyring = "4"` until 4.0.0 ships and the changelog is reviewed — major versions of keyring have historically introduced breaking platform-feature reorganizations (the 2 → 3 transition, for example, removed the implicit default platform feature).
- **`oauth2` crate version drift.** This walkthrough assumes the `oauth2` crate version currently in `inboxly-imap`'s `Cargo.toml`. If Phase 2 also bumps `oauth2`, re-grep `oauth2.rs` for `TokenResponse` / `RefreshToken` API shape — the trait moved between `oauth2 = "4"` and `oauth2 = "5"` and may move again.
- **`open::that` in `authorize`** (line 122). Same crate that caused the M34 side-effecting-tests incident. Phase 1 + Phase 2 secrets-store tests must NEVER call `authorize` directly; they must only call the persistence wrapper with a pre-built `OAuth2Token`. Add a comment to the new wrapper function reminding future contributors.
- **Refresh token rotation on `refresh_token` calls.** Already noted: re-persist after every successful refresh, even if the value didn't change. Idempotent and cheap. Tests should round-trip a refresh that returns the SAME refresh token and confirm the keyring entry is rewritten (not skipped).
- **Multi-account `email` field canonicalization.** The user-field naming scheme requires ASCII-lowercased email. Inboxly's `Account` model probably already lowercases on insert (Phase 1 should verify), but if it doesn't, the secrets-store wrapper MUST do its own lowercasing or two `Account` rows that differ only in case will collide on the keyring side and silently overwrite each other.
- **No keyring code yet — Phase 0 ends here.** Phase 1 wires `keyring.workspace = true` into the new `inboxly-secrets` crate (or wherever the secrets backend lives), defines the `SecretsStore` trait + `MemorySecretsStore` + `KeyringSecretsStore`, and ships unit tests using only `MemorySecretsStore`. Phase 2 plumbs `SecretsStore` into the OAuth2 dispatcher and the password auth path.
