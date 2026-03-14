# Inbox by Google — Complete Design Reference

## Sources
- APK decompilation: `com.google.android.apps.inbox` v1.78.217178463 (internal codename: **BigTop**, prefix: `bt_`)
- Open-source recreations: pinbox, inbox-reborn, inboxy, material-inbox
- UX analyses, design articles, and post-mortems

---

## 1. Colour System

### Primary Brand
| Name | Value | Purpose |
|------|-------|---------|
| `bt_container_blue` | `#4285f4` | Primary brand blue — toolbar, FAB |
| `bt_blue` | `#3c80f6` | Accent colour |
| `bt_status_bar_blue` | `#3367d6` | Status bar (Inbox view) |

### View State Colours (the 3 views)
| View | Toolbar | Status Bar |
|------|---------|------------|
| Inbox | `#4285f4` (blue) | `#3367d6` |
| Done | `#0f9d58` (green) | `#0b8043` |
| Snoozed | `#ef6c00` (orange) | `#c65900` |

### Bundle/Category Colours
| Bundle | Title Colour | Badge Background |
|--------|-------------|-----------------|
| Social | `#d23f31` (red) | `#faebea` |
| Promos | `#00acc1` (cyan) | `#e5f6f9` |
| Updates | `#f4511e` (deep orange) | `#feede8` |
| Finance | `#558b2f` (green) | `#eef3ea` |
| Purchases | `#6d4c41` (brown) | `#f0edec` |
| Travel | `#8e24aa` (purple) | `#f3e9f6` |
| Forums | `#3949ab` (indigo) | `#ebecf6` |
| Saved | `#3367d6` (blue) | — |
| Custom | `#212121` (dark) | — |
| Low Priority | `#212121` (dark) | `#e5e5e5` |

### Letter Tile Avatar Colours (A-Z)
```
A=#e06055  B=#ed6192  C=#ba68c8  D=#9575cd  E=#7986cb  F=#5e97f6  G=#4fc3f7
H=#58d0e1  I=#4fb6ac  J=#57bb8a  K=#9ccc65  L=#d4e157  M=#fdd835  N=#f6bf32
O=#f5a631  P=#f18864  Q=#c2c2c2  R=#90a4ae  S=#a1887f  T=#a3a3a3  U=#afb6e0
V=#b39ddb  W=#c2c2c2  X=#80deea  Y=#bcaaa4  Z=#aed581  default=#efefef
```

### Text Colours
| Name | Value | Purpose |
|------|-------|---------|
| `bt_dark_text` | `#212121` | Primary text |
| `bt_faint_text` | `#757575` | Secondary/snippet text |
| `bt_white_text` | `#ffffff` | Text on coloured backgrounds |

### Background Colours
| Name | Value | Purpose |
|------|-------|---------|
| `bt_megalist_background` | `#ececec` | Main list background |
| `bt_megalist_item_background` | `#ffffff` | Card/item background |
| `bt_megalist_item_background_selected` | `#ebf2ff` | Selected item |
| `bt_light_grey` | `#f6f6f6` | Light grey surfaces |
| `bt_stroke_grey` | `#e0e0e0` | Dividers/strokes |

### Onboarding Feature Colours
| Feature | Background |
|---------|-----------|
| Bundles | `#689f38` (green) |
| Reminders | `#4285f4` (blue) |
| Smart Mail | `#7e57c2` (purple) |
| Snooze | `#ef6c00` (orange) |

---

## 2. Typography

| Element | Size | Weight | Font |
|---------|------|--------|------|
| Toolbar title | 20sp | Normal | sans-serif |
| Email title/sender | 16sp | Normal | sans-serif |
| Author name | 14sp | Normal | sans-serif |
| Snippet/preview | 14sp | Normal | sans-serif |
| Timestamp | 12sp | Normal | sans-serif |
| Section header | 14sp | Bold | sans-serif |
| Unread count badge | 16sp | Bold | sans-serif |
| Compose subject | 18sp | Bold | sans-serif |
| Compose body | 16sp | Normal | sans-serif |
| Dialog title | 20sp | Normal | sans-serif |
| Nav drawer items | 14sp | Medium | sans-serif-medium |
| Labels/actions | 14sp | Medium | sans-serif-medium |
| Speed dial items | 14sp | Bold | sans-serif |

---

## 3. Dimensions & Spacing

### Global Layout
| Token | Value |
|-------|-------|
| Default margin/padding | 16dp |
| Toolbar height | 56dp |
| Toolbar elevation | 2dp |
| Nav drawer width | 264dp |
| Nav drawer item height | 48dp |
| FAB margin from edges | 13dp |
| Divider thickness | 1px |

### Conversation List Items
| Token | Value |
|-------|-------|
| Avatar circle diameter | 40dp |
| Avatar column width (with padding) | 72dp |
| List item horizontal padding | 16dp |
| Text column start keyline | 72dp |
| First text line height | 24dp |
| Subsequent line height | 20dp |
| Pin/state icon width | 18dp |
| Card elevation | 2dp |
| Card corner radius | **0dp** (flat!) |
| Section header height | 48dp |

### Snooze Grid
| Token | Value |
|-------|-------|
| Grid width | 288dp |
| Default option size | 142dp × 122dp |
| Custom option size | 142dp × 100dp |
| Grid spacing | 4dp |
| Columns | 2 |

### Speed Dial
| Token | Value |
|-------|-------|
| Item height | 56dp |
| Icon diameter | 56dp |

### Compose
| Token | Value |
|-------|-------|
| Max width | 920dp |
| From row height | 56dp |
| Contacts row height | 56dp |

---

## 4. Core Systems

### "Done" = Archive
From decompiled source: `ARCHIVE(R.string.bt_action_mark_as_done)`.
- "Done" is a relabeling of Gmail's archive action
- Green checkmark icon, green toolbar when viewing Done
- Sweep = "Clear unpinned" (bulk archive all unpinned in a section)

### Bundle/Cluster System
Internal term: "cluster". User-facing: "bundle".

**Priority weights** (higher = shown first):
- Primary Inbox: 15400
- Social: 15300, Promos: 15200, Updates: 15100, Forums: 15000
- Custom bundles: 10000
- Trips: 5000
- Travel: 1700, Purchases: 1600, Finance: 1500, Social: 1400
- Updates: 1300, Forums: 1200, Promos: 1100, Low Priority: 1000

**Visibility modes**: Bundled in inbox | Unbundled | Skip the inbox

**Throttling**: As messages arrive | Once a day | Once a week

### Pin System
- Boolean attribute on conversations
- Pinned items stay at top of inbox
- Excluded from sweep ("Clear unpinned")
- `PIN` (13) and `REMOVE_PIN` (14) server operations

### Snooze System
**Time options**: Later Today, Tomorrow, This Weekend, Next Week, Someday, Custom Date/Time
**Location options**: Pick Place (geofence-based)
**Day-specific**: Morning, Afternoon, Evening, Night
**Special**: Repeat last snooze, Smart time suggestion, Unsnooze

### Reminder System
- Internal name: "Tasks"
- User-facing: "Reminders"
- Input hint: "Remember to..."
- Created via speed dial FAB (alongside Compose)
- Appear in inbox feed alongside emails
- Support time-based and location-based triggers
- Integration with Google Keep and Calendar

### Highlights
- ML-based priority email surfacing
- Trainable via yes/no/skip per email
- Shows high-priority emails first

### Time Sections
Items grouped by: PINNED (top), DAY, WEEK, MONTH, EARLIER, LATER, SOMEDAY

---

## 5. Layout Architecture

### Main Screen
```
DrawerLayout
  FrameLayout
    DraggableLayout (fragment_holder)
    Toolbar (56dp, blue/green/orange by view)
    FAB (speed dial: Compose + Reminder)
  Nav Drawer (264dp)
```

### Conversation List Item
```
CoreTlItemViewLayout (swipeable container)
  CoreTlView (custom layout)
    Avatar (40dp circle, 72dp column)
    Title (16sp, first line)
    Date (hidden by default)
    New badge (blue pill)
    State icon (pin, 18dp)
    Attachment indicator
    Source name (14sp, authors)
    Updates (RecyclerView)
    Snooze status
    Snippet + labels
    Reminder text (max 3 lines)
    Smart mail container
```

### Navigation Drawer
Primary: Inbox, Snoozed, Done
Secondary: Drafts, Sent, Reminders, Trash, Spam
Footer: Settings, Help

---

## 6. Interaction Design

### Swipe Gestures
- **Right swipe**: Mark as Done (green background, checkmark icon)
- **Left swipe**: Snooze (yellow/amber background, clock icon)
- Two-threshold model: ~25% arms the action, ~50% commits
- Elastic snapback if reversed before commit
- Haptic feedback at commit threshold

### Bundle Expand/Collapse
- In-place expansion (not navigation)
- Items above slide up, items below slide down
- Container transform animation (250-300ms)
- Surrounding items maintain spatial context

### Desktop Hover Actions
- Hovering reveals: Done (checkmark), Snooze (clock), Pin (thumbtack)
- Hidden by default for clean scanning

### Empty Inbox State
- Sun illustration (yellow sun, blue sky gradient)
- Emotional reward for inbox zero
- Only appears when zero unpinned items remain

### View Transitions
- Inbox → email: container transform (row expands to fill)
- FAB → compose: FAB expands into compose screen
- Swipe dismiss: row slides off, below item slides up (~200ms)
- Sweep: cascading collapse, ~50ms stagger per row

---

## 7. Reference Implementations

### pinbox (Go + Angular)
- Architecture: Client → Go API → Notmuch → OfflineIMAP
- Bundling via notmuch tags (manual, config-defined)
- Read-only (no archive/pin/snooze functionality)
- Lazy message loading on expand
- Key lesson: Notmuch/Maildir as backend is viable

### inbox-reborn (Chrome Extension)
- Most complete Inbox recreation
- Bundling by Gmail labels, navigate to search on click
- Date grouping (Today/Yesterday/This Month/older)
- 26-colour avatar palette
- BIMI brand logo fetching
- Dark mode support
- Context-coloured header bar
- Inbox zero sun illustration
- Key CSS: bg `#f2f2f2`, cards white, max-width 1200px, FAB 56px red

### inboxy (Chrome Extension)
- Most architecturally clean implementation
- In-place bundle expansion via CSS flexbox order
- Bulk archive via programmatic checkbox selection
- Pinned toggle (star filter)
- Key lesson: flex-direction + order property for expand without DOM mutation

### material-inbox (Angular)
- UI shell/prototype only, no backend
- Canonical navigation hierarchy
- Empty state patterns with large circular icons
- Email as expansion panel pattern
- Hover action buttons (snooze/delete/done)

---

## 8. What Gmail Adopted vs What Was Lost

### Adopted by Gmail
- Smart Reply (3 contextual quick replies)
- Snooze (time-based only, no location)
- Nudges (reminders about old emails)
- Some UI polish (Material Design updates)

### Lost When Inbox Died (2019)
- Automatic email bundling/categorization
- In-place bundle expand/collapse
- "Done" framing (satisfying vs neutral archive)
- Sweep (bulk archive unpinned)
- Pinning (keep important items visible during sweep)
- Location-based snooze
- Reminders mixed into email feed
- Trip bundles with hero images
- Highlights (inline extracted info)
- Save to Inbox (URL bookmarking)
- Inbox zero sun illustration
- Speed dial FAB (compose + reminder)
- Bundle delivery scheduling (daily/weekly digest)
