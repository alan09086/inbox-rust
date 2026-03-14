# Inboxly — Implementation Roadmap

> **25 milestones**, each with its own detailed implementation plan.
> Plans are written upfront before any implementation begins.

**Spec:** `docs/superpowers/specs/2026-03-14-inboxly-design.md`

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
```

## Key Checkpoints

- **After M5**: Storage engine complete — fully testable with fixture emails
- **After M9**: Can sync a real IMAP mailbox to local storage
- **After M14**: Complete backend engine (sync + store + bundler)
- **After M17**: First visual — emails on screen
- **After M19**: Usable email client (read + archive + pin)
- **After M25**: Full Inboxly v1

## Licence

GPL-3.0
