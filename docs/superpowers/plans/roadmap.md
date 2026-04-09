# Inboxly — Implementation Roadmap

> **42 milestones (M1–M42)**, each with its own detailed implementation plan.
> Plans are written upfront before any implementation begins.

**Specs:**
- `docs/superpowers/specs/2026-03-14-inboxly-design.md` (v1 core)
- `docs/superpowers/specs/2026-03-14-qol-menus-settings-design.md` (QoL: menus, settings, dropdowns)
- `docs/superpowers/specs/2026-04-06-inboxly-v2-full-client-design.md` (v2 daily-driver client)

**Numbering note:** The v2 design spec (`8e49fd9`) was published with milestones labelled M31–M40. After M31 shipped, the project took a two-milestone framework-conversion detour (Iced → Dioxus) that was not in any spec — the actual M32 and M33 in git history are that detour and the widget restoration that followed it. The v2 spec's M32 onwards is therefore renumbered as M34 onwards in this roadmap. The v2 spec content stays the source of truth for the **what**; this roadmap is the source of truth for the **when** and **where in git history**.

---

## Milestone Index

| # | Name | Crate(s) | Plan |
|---|------|----------|------|
| M1 | Workspace + Core Types | `core` | [M1 Plan](./2026-03-14-m01-workspace-core-types.md) |
| M2 | Config System | `core` | [M2 Plan](./2026-03-14-m02-config-system.md) |
| M3 | SQLite Schema + Store API | `store` | [M3 Plan](./2026-03-14-m03-sqlite-store-api.md) |
| M4 | Maildir++ Operations | `store` | [M4 Plan](./2026-03-14-m04-maildir-operations.md) |
| M5 | Tantivy Search Index | `store` | [M5 Plan](./2026-03-14-m05-tantivy-index.md) |
| M6 | IMAP Connection + Auth | `imap` | [M6 Plan](./2026-03-14-m06-imap-connection-auth.md) |
| M7 | Initial Sync Phase 1 | `imap` | [M7 Plan](./2026-03-14-m07-initial-sync-phase1.md) |
| M8 | Initial Sync Phase 2 | `imap` | [M8 Plan](./2026-03-14-m08-initial-sync-phase2.md) |
| M9 | Incremental Sync + IDLE | `imap` | [M9 Plan](./2026-03-14-m09-incremental-sync-idle.md) |
| M10 | Threading Algorithm | `store` | [M10 Plan](./2026-03-14-m10-threading-algorithm.md) |
| M11 | Contacts + Avatar System | `store` | [M11 Plan](./2026-03-14-m11-contacts-avatars.md) |
| M12 | Bundler: Header Heuristics | `bundler` | [M12 Plan](./2026-03-14-m12-bundler-heuristics.md) |
| M13 | Bundler: User Rules + Learning | `bundler` | [M13 Plan](./2026-03-14-m13-bundler-rules-learning.md) |
| M14 | Bundle Throttling | `bundler` | [M14 Plan](./2026-03-14-m14-bundle-throttling.md) |
| M15 | Iced Shell + Nav Drawer | `ui`, binary | [M15 Plan](./2026-03-14-m15-iced-shell-nav.md) |
| M16 | Theme System | `ui` | [M16 Plan](./2026-03-14-m16-theme-system.md) |
| M17 | Inbox Feed + Email Rows | `ui` | [M17 Plan](./2026-03-14-m17-inbox-feed-rows.md) |
| M18 | Bundle Rows + Expand/Collapse | `ui` | [M18 Plan](./2026-03-14-m18-bundle-rows.md) |
| M19 | Done + Pin + Sweep + Undo | `ui`, `store` | [M19 Plan](./2026-03-14-m19-done-pin-sweep.md) |
| M20 | Swipe + Hover Actions | `ui` | [M20 Plan](./2026-03-14-m20-swipe-hover.md) |
| M21 | Snooze + Picker | `snooze`, `ui` | [M21 Plan](./2026-03-14-m21-snooze-picker.md) |
| M22 | Reminders + Speed Dial FAB | `snooze`, `ui` | [M22 Plan](./2026-03-14-m22-reminders-fab.md) |
| M23 | Compose + SMTP + Drafts | `imap`, `ui` | [M23 Plan](./2026-03-14-m23-compose-smtp.md) |
| M24 | Search + Query Parser | `store`, `ui` | [M24 Plan](./2026-03-14-m24-search-query.md) |
| M25 | Highlights + Trips + Multi-Account + Polish | `extract`, `imap`, `store`, `ui` | [M25 Plan](./2026-03-14-m25-highlights-polish.md) |
| | **Post-v1: QoL, Menus & Settings** | | |
| M26 | PopupMenu Widget | `ui` | [M26 Plan](./2026-03-14-m26-popup-menu-widget.md) |
| M27 | Overflow + Context Menus | `ui` | [M27 Plan](./2026-03-14-m27-overflow-context-menus.md) |
| M28 | Account Switcher | `ui` | [M28 Plan](./2026-03-14-m28-account-switcher.md) |
| M29 | Settings: General + Accounts + Data | `ui`, `core`, `store` | [M29 Plan](./2026-03-14-m29-settings-general-accounts.md) |
| M30 | Settings: Bundles + Notifications + Shortcuts | `ui`, `store`, `bundler` | [M30 Plan](./2026-03-14-m30-settings-bundles-shortcuts.md) |
| | **v2: Daily-driver client** (post-Dioxus, source spec: `2026-04-06-inboxly-v2-full-client-design.md`) | | |
| M31 ✅ | Store Trait Integration + IMAP Action Execution | `store`, `imap`, `ui` | [M31 Plan](./2026-04-06-m31-store-traits-action-execution.md) |
| M32 ✅ | Iced → Dioxus Framework Conversion (unplanned detour) | `ui` (whole shell) | _no plan file — direct conversion_ |
| M33 ✅ | Inbox Feed Widgets on Dioxus | `ui` | [M33 Plan](./2026-04-06-m33-inbox-feed-widgets.md) |
| M34 ✅ | Thread Detail View + HTML Email Rendering | `ui` | _v2 spec §M32 (adapted for Dioxus — no wry needed)_ |
| M35 ✅ | SMTP Engine + Compose View on Dioxus | `imap`, `ui` | _v2 spec §M33 (compose view created from scratch)_ |
| M36 ✅ | Reply + Reply All + Forward | `imap`, `ui` | _v2 spec §M34_ |
| M37 | Full Attachment Support | `imap`, `store`, `ui` | _v2 spec §M35_ |
| M38 | Advanced Search | `store`, `ui` | _v2 spec §M36_ |
| M39 | End-to-End Bundling | `bundler`, `imap`, `ui` | _v2 spec §M37_ |
| M40 | Snooze System (real wakeups) | `snooze`, `imap`, `ui` | _v2 spec §M38_ |
| M41 | Smart Highlights & Extraction | `extract`, `store`, `ui` | _v2 spec §M39_ |
| M42 | Integration Polish & First Run | `core`, `ui` | _v2 spec §M40_ |

## Dependency Graph

```
M1 → M2 → M3 → M4 → M5
                ↓
M6 → M7 → M8 → M9
      ↓
      M10 → M11
             ↓
M12 → M13 → M14
             ↓
M15 → M16 → M17 → M18
                    ↓
M19 → M20
       ↓
M21 → M22
       ↓
M23 → M24 → M25
                 ↓
M26 → M27
       ↓
       M28
       ↓
M29 → M30
       ↓
M31 → M32 (framework detour) → M33 (widget restoration on Dioxus)
                                           ↓
                                          M34 → M35 → M36
                                                       ↓
                                                      M37
                                                       ↓
                                                      M38
                                                       ↓
                                                      M39
                                                       ↓
                                                      M40
                                                       ↓
                                                      M41
                                                       ↓
                                                      M42
```

**M34 → M35 → M36 chain notes:** Reply/Reply All/Forward (M36) requires both the thread detail view (M34) — to know which thread the user is replying from — and the SMTP/compose plumbing (M35). Attachments (M37) build on M34's HTML rendering for inline previews.

## Key Checkpoints

- **After M5**: Storage engine complete — fully testable with fixture emails
- **After M9**: Can sync a real IMAP mailbox to local storage
- **After M14**: Complete backend engine (sync + store + bundler)
- **After M17**: First visual — emails on screen
- **After M19**: Usable email client (read + archive + pin)
- **After M25**: Full Inboxly v1
- **After M27**: Contextual menus working (overflow + right-click)
- **After M30**: Polished desktop client with full settings
- **After M31**: Triage actions actually sync to the IMAP server
- **After M33**: Inbox feed restored on Dioxus (post-framework-conversion baseline)
- **After M34**: User can read full email content (thread detail view)
- **After M36**: Full bidirectional email client — read AND write/reply
- **After M40**: Snoozes actually wake up at the right time
- **After M42**: Daily-driver release candidate

## Licence

GPL-3.0
