# M20: Swipe + Hover Actions — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan.

**Goal:** Implement SwipeContainer custom widget wrapping inbox rows, providing mouse-drag swipe gestures (right = Done, left = Snooze) with two-threshold arm/commit system, plus hover-reveal action buttons as the primary desktop interaction path.

**Architecture:** A single `SwipeContainer<'a, Message>` widget in `inboxly-ui` that wraps any inbox row widget. It owns all swipe/hover state and emits action messages (`Done`, `Snooze`, `Pin`) to the application. The widget implements Iced's `Widget` trait directly (`layout()`, `draw()`, `on_event()`, `mouse_interaction()`). Animation state lives in a companion `SwipeState` struct stored in the application model, keyed per row.

**Tech Stack:** Rust, iced (0.13+), iced_core (Widget trait, renderer, event handling)

**Prerequisite:** M19 complete — `MarkDone`, `Pin`, `Unpin`, `Snooze` messages exist in the application `Message` enum. `EmailRow` widget renders inbox rows. Theme system provides colour tokens. The application model has a working inbox feed with clickable rows.

---

## Task 1: Define SwipePhase, HoverState, and SwipeState types

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/swipe_state.rs` (new file)

Create the state types that track per-row swipe and hover state. These are stored in the application model (one `SwipeState` per visible inbox row, keyed by `ThreadId` or `InboxItemId`).

```rust
use std::time::Instant;

/// Which direction the user is dragging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwipeDirection {
    /// Right drag = Done (green + checkmark).
    Right,
    /// Left drag = Snooze (orange + clock).
    Left,
}

/// The phase of a swipe gesture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SwipePhase {
    /// No active swipe. Row is at rest (offset = 0).
    Idle,
    /// User is actively dragging. Stores the current horizontal pixel offset
    /// (positive = right, negative = left).
    Dragging { offset: f32 },
    /// Drag released before commit threshold. Animating back to offset 0.
    /// `start_offset` is where snapback began, `started_at` is the instant.
    SnapBack {
        start_offset: f32,
        started_at: Instant,
    },
    /// Commit threshold reached. Row is animating off-screen.
    /// `direction` is which side it exits toward. `started_at` is the instant.
    Committing {
        direction: SwipeDirection,
        started_at: Instant,
    },
    /// Row has exited. Gap is collapsing (height animating to 0).
    Collapsing {
        started_at: Instant,
        original_height: f32,
    },
    /// Animation complete. The parent should remove this row from the feed.
    Done,
}

/// Per-row state for the SwipeContainer widget.
#[derive(Debug, Clone)]
pub struct SwipeState {
    /// Current phase of the swipe gesture / animation.
    pub phase: SwipePhase,
    /// Whether the mouse is currently hovering over this row.
    pub hovered: bool,
    /// The row width at last layout, used to compute thresholds.
    pub row_width: f32,
}

impl Default for SwipeState {
    fn default() -> Self {
        Self {
            phase: SwipePhase::Idle,
            hovered: false,
            row_width: 0.0,
        }
    }
}

impl SwipeState {
    /// Arm threshold: 25% of row width. When |offset| exceeds this,
    /// the action icon appears and background colour intensifies.
    pub fn arm_threshold(&self) -> f32 {
        self.row_width * 0.25
    }

    /// Commit threshold: 50% of row width. When |offset| exceeds this
    /// on mouse release, the action fires.
    pub fn commit_threshold(&self) -> f32 {
        self.row_width * 0.50
    }

    /// Returns the current swipe direction based on the drag offset, or None if idle.
    pub fn direction(&self) -> Option<SwipeDirection> {
        match self.phase {
            SwipePhase::Dragging { offset } => {
                if offset > 0.0 {
                    Some(SwipeDirection::Right)
                } else if offset < 0.0 {
                    Some(SwipeDirection::Left)
                } else {
                    None
                }
            }
            SwipePhase::Committing { direction, .. } => Some(direction),
            _ => None,
        }
    }

    /// Whether the swipe has passed the arm threshold (icon should be visible).
    pub fn is_armed(&self) -> bool {
        match self.phase {
            SwipePhase::Dragging { offset } => offset.abs() >= self.arm_threshold(),
            _ => false,
        }
    }

    /// Whether the swipe has passed the commit threshold.
    pub fn past_commit(&self) -> bool {
        match self.phase {
            SwipePhase::Dragging { offset } => offset.abs() >= self.commit_threshold(),
            _ => false,
        }
    }
}
```

**Also:** Add `pub mod swipe_state;` to `inboxly-ui/src/widgets/mod.rs`. If `widgets/mod.rs` does not exist, create it and add the module, then add `pub mod widgets;` to `inboxly-ui/src/lib.rs`.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add SwipeState, SwipePhase, and SwipeDirection types for swipe gestures`

---

## Task 2: Define SwipeAction enum and animation constants

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/swipe_state.rs` (append)

Add the action enum that SwipeContainer emits, plus timing/easing constants. These are separate from the application's top-level `Message` enum -- the SwipeContainer maps these to app messages via a closure.

```rust
use std::time::Duration;

/// Action that the SwipeContainer can trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwipeAction {
    /// Mark the item as done (archive). Triggered by right swipe past commit threshold.
    Done,
    /// Open the snooze picker. Triggered by left swipe past commit threshold.
    Snooze,
    /// Pin/unpin the item. Triggered by hover action button.
    TogglePin,
}

/// Animation timing constants.
pub mod animation {
    use std::time::Duration;

    /// Duration of the snapback animation when drag is released before commit.
    /// Matches BigTop's elastic feel.
    pub const SNAPBACK_DURATION: Duration = Duration::from_millis(250);

    /// Duration of the commit slide-off animation. The row slides fully
    /// off-screen in this time.
    pub const COMMIT_SLIDE_DURATION: Duration = Duration::from_millis(150);

    /// Duration of the gap collapse animation after the row exits.
    pub const COLLAPSE_DURATION: Duration = Duration::from_millis(200);

    /// Minimum horizontal drag distance (in logical pixels) before a swipe
    /// is recognised. Prevents accidental swipes from vertical scrolling.
    pub const DRAG_DEAD_ZONE: f32 = 8.0;

    /// Easing function for snapback: decelerate (ease-out quadratic).
    /// Returns a value in [0.0, 1.0] for t in [0.0, 1.0].
    pub fn ease_out_quad(t: f32) -> f32 {
        1.0 - (1.0 - t) * (1.0 - t)
    }

    /// Easing function for commit slide: accelerate (ease-in quadratic).
    pub fn ease_in_quad(t: f32) -> f32 {
        t * t
    }
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add SwipeAction enum and animation timing constants`

---

## Task 3: Create SwipeContainer widget struct and constructor

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/swipe_container.rs` (new file)

Create the `SwipeContainer` widget struct. This is a wrapper widget (like Iced's `Container`) that wraps a child element. It takes closures to map `SwipeAction` values to the application's `Message` type.

```rust
use iced::advanced::widget::Widget;
use iced::advanced::{layout, renderer, Clipboard, Layout, Shell};
use iced::{Element, Event, Length, Point, Rectangle, Size, Vector};

use super::swipe_state::{SwipeAction, SwipeDirection, SwipePhase, SwipeState};

/// A container widget that wraps an inbox row and provides:
/// - Mouse click-and-drag horizontal swipe (right = Done, left = Snooze)
/// - Hover-reveal action buttons (Done, Snooze, Pin)
///
/// # Type Parameters
/// - `'a`: lifetime of the child element
/// - `Message`: the application message type
/// - `Theme`: the application theme type
/// - `Renderer`: the renderer type
pub struct SwipeContainer<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    /// The wrapped child element (e.g., an EmailRow).
    child: Element<'a, Message, Theme, Renderer>,
    /// Reference to the per-row swipe state, stored in the app model.
    state: &'a SwipeState,
    /// Callback that maps a SwipeAction to an application Message.
    on_action: Box<dyn Fn(SwipeAction) -> Message + 'a>,
    /// Callback for swipe state mutations. The widget cannot mutate state
    /// directly in Iced's elm architecture; it emits messages that the
    /// application update() uses to mutate SwipeState.
    on_state_change: Box<dyn Fn(SwipePhase) -> Message + 'a>,
    /// Width of the container (defaults to Fill).
    width: Length,
    /// Height of the container (defaults to Shrink, matching the child).
    height: Length,
}

impl<'a, Message, Theme, Renderer> SwipeContainer<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    /// Create a new SwipeContainer wrapping the given child element.
    ///
    /// - `child`: the inbox row widget to wrap
    /// - `state`: reference to this row's SwipeState from the app model
    /// - `on_action`: maps committed SwipeActions to app Messages
    /// - `on_state_change`: maps SwipePhase transitions to app Messages
    pub fn new(
        child: impl Into<Element<'a, Message, Theme, Renderer>>,
        state: &'a SwipeState,
        on_action: impl Fn(SwipeAction) -> Message + 'a,
        on_state_change: impl Fn(SwipePhase) -> Message + 'a,
    ) -> Self {
        Self {
            child: child.into(),
            state,
            on_action: Box::new(on_action),
            on_state_change: Box::new(on_state_change),
            width: Length::Fill,
            height: Length::Shrink,
        }
    }

    /// Set the width of the container.
    pub fn width(mut self, width: Length) -> Self {
        self.width = width;
        self
    }

    /// Set the height of the container.
    pub fn height(mut self, height: Length) -> Self {
        self.height = height;
        self
    }
}
```

**Also:** Add `pub mod swipe_container;` to `inboxly-ui/src/widgets/mod.rs`.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add SwipeContainer widget struct with constructor`

---

## Task 4: Implement Widget::layout() for SwipeContainer

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/swipe_container.rs` (append)

Implement the `Widget` trait, starting with `layout()`. The SwipeContainer delegates layout to its child. During the `Collapsing` phase, the container height is animated down to 0. This is the only phase where layout differs from the child's natural size.

```rust
use std::time::Instant;

use super::swipe_state::animation;

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for SwipeContainer<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    fn size(&self) -> Size<Length> {
        Size::new(self.width, self.height)
    }

    fn layout(
        &self,
        tree: &mut iced::advanced::widget::Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        // Delegate layout to the child.
        let child_limits = limits.width(self.width).height(self.height);
        let mut child_node = self.child
            .as_widget()
            .layout(&mut tree.children[0], renderer, &child_limits);

        let child_size = child_node.size();

        // During Collapsing phase, interpolate height from original to 0.
        let container_height = match self.state.phase {
            SwipePhase::Collapsing {
                started_at,
                original_height,
            } => {
                let elapsed = started_at.elapsed().as_secs_f32();
                let duration = animation::COLLAPSE_DURATION.as_secs_f32();
                let t = (elapsed / duration).min(1.0);
                let eased = animation::ease_out_quad(t);
                original_height * (1.0 - eased)
            }
            SwipePhase::Done => 0.0,
            _ => child_size.height,
        };

        // Position the child at (0, 0) within the container.
        child_node = child_node.move_to(Point::ORIGIN);

        layout::Node::with_children(
            Size::new(child_size.width, container_height),
            vec![child_node],
        )
    }

    fn children(&self) -> Vec<iced::advanced::widget::Tree> {
        vec![iced::advanced::widget::Tree::new(&self.child)]
    }

    fn diff(&self, tree: &mut iced::advanced::widget::Tree) {
        tree.diff_children(std::slice::from_ref(&self.child));
    }
```

**Note:** The `layout()`, `children()`, and `diff()` methods are part of the `Widget` impl block that will be completed in subsequent tasks. The closing `}` for the impl block is NOT placed here — it continues in Tasks 5, 6, and 7.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement Widget::layout() for SwipeContainer with collapse animation`

---

## Task 5: Implement Widget::on_event() for SwipeContainer — drag detection

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/swipe_container.rs` (append to Widget impl)

Implement `on_event()` — the core interaction handler. This method handles:
1. **Mouse enter/leave** — toggle `hovered` state
2. **Mouse press** — record drag start position
3. **Mouse move** — if dragging, compute horizontal offset, emit `SwipePhase::Dragging`
4. **Mouse release** — if past commit threshold, emit action + `SwipePhase::Committing`; otherwise emit `SwipePhase::SnapBack`

The drag dead zone (8px) prevents accidental swipes during vertical scrolling. Once horizontal drag exceeds the dead zone, vertical scroll is suppressed.

```rust
    fn on_event(
        &mut self,
        tree: &mut iced::advanced::widget::Tree,
        event: Event,
        layout: Layout<'_>,
        cursor: iced::advanced::mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) -> iced::event::Status {
        use iced::event::Status;
        use iced::mouse;

        let bounds = layout.bounds();

        // Track hover state.
        match &event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let is_over = cursor.is_over(bounds);
                if is_over != self.state.hovered {
                    // We cannot mutate state directly. Emit a message
                    // via on_state_change. The app's update() handles
                    // setting `state.hovered = is_over`.
                    // For hover, we keep the same phase but the app
                    // must track hover separately via a dedicated message.
                    // This is handled by the parent emitting a hover message.
                }
            }
            _ => {}
        }

        // Delegate non-drag events to child when idle and not dragging.
        // During active drag, we consume all mouse events.
        let child_layout = layout.children().next().unwrap();

        match self.state.phase {
            SwipePhase::Idle => {
                // In idle, handle drag initiation on mouse press.
                match &event {
                    Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                        if cursor.is_over(bounds) {
                            // Record drag start. Transition to Dragging with offset 0.
                            // The actual drag origin (cursor X position) must be stored.
                            // We use the internal widget state tree for mutable per-instance state.
                            let state = tree.state.downcast_mut::<SwipeDragState>();
                            if let Some(position) = cursor.position() {
                                state.drag_origin = Some(position.x);
                                state.drag_started = false;
                            }
                            // Don't consume yet — let child handle click if no drag develops.
                            return Status::Ignored;
                        }
                    }
                    Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                        let state = tree.state.downcast_mut::<SwipeDragState>();
                        if let (Some(origin), Some(position)) =
                            (state.drag_origin, cursor.position())
                        {
                            let dx = position.x - origin;
                            if dx.abs() > animation::DRAG_DEAD_ZONE && !state.drag_started {
                                // Horizontal drag exceeds dead zone — start swipe.
                                state.drag_started = true;
                                shell.publish((self.on_state_change)(
                                    SwipePhase::Dragging { offset: dx },
                                ));
                                return Status::Captured;
                            } else if state.drag_started {
                                shell.publish((self.on_state_change)(
                                    SwipePhase::Dragging { offset: dx },
                                ));
                                return Status::Captured;
                            }
                        }
                    }
                    Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                        let state = tree.state.downcast_mut::<SwipeDragState>();
                        state.drag_origin = None;
                        state.drag_started = false;
                    }
                    _ => {}
                }

                // Delegate to child.
                self.child.as_widget_mut().on_event(
                    &mut tree.children[0],
                    event,
                    child_layout,
                    cursor,
                    renderer,
                    clipboard,
                    shell,
                    viewport,
                )
            }
            SwipePhase::Dragging { offset } => {
                match &event {
                    Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                        let state = tree.state.downcast_mut::<SwipeDragState>();
                        if let (Some(origin), Some(position)) =
                            (state.drag_origin, cursor.position())
                        {
                            let dx = position.x - origin;
                            shell.publish((self.on_state_change)(
                                SwipePhase::Dragging { offset: dx },
                            ));
                        }
                        Status::Captured
                    }
                    Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                        let state = tree.state.downcast_mut::<SwipeDragState>();
                        state.drag_origin = None;
                        state.drag_started = false;

                        // Decide: commit or snapback?
                        if self.state.past_commit() {
                            let direction = if offset > 0.0 {
                                SwipeDirection::Right
                            } else {
                                SwipeDirection::Left
                            };
                            // Fire the action.
                            let action = match direction {
                                SwipeDirection::Right => SwipeAction::Done,
                                SwipeDirection::Left => SwipeAction::Snooze,
                            };
                            shell.publish((self.on_action)(action));
                            // Start commit animation.
                            shell.publish((self.on_state_change)(
                                SwipePhase::Committing {
                                    direction,
                                    started_at: Instant::now(),
                                },
                            ));
                        } else {
                            // Snapback.
                            shell.publish((self.on_state_change)(
                                SwipePhase::SnapBack {
                                    start_offset: offset,
                                    started_at: Instant::now(),
                                },
                            ));
                        }
                        Status::Captured
                    }
                    _ => Status::Captured, // Consume all events during drag.
                }
            }
            // During animation phases, consume mouse events but don't process them.
            SwipePhase::SnapBack { .. }
            | SwipePhase::Committing { .. }
            | SwipePhase::Collapsing { .. }
            | SwipePhase::Done => Status::Ignored,
        }
    }
```

**Also:** Define the internal mutable drag state struct (stored in the widget tree, not in the app model):

```rust
/// Internal mutable state stored in the Iced widget tree.
/// Tracks the drag origin point for the current gesture.
#[derive(Debug, Default)]
struct SwipeDragState {
    /// X coordinate where the mouse was pressed to start a potential drag.
    drag_origin: Option<f32>,
    /// Whether the drag dead zone has been exceeded (swipe is active).
    drag_started: bool,
}
```

Add the `state()` method to the Widget impl to register this internal state:

```rust
    fn state(&self) -> iced::advanced::widget::tree::State {
        iced::advanced::widget::tree::State::new(SwipeDragState::default())
    }

    fn tag(&self) -> iced::advanced::widget::tree::Tag {
        iced::advanced::widget::tree::Tag::of::<SwipeDragState>()
    }
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement on_event() for SwipeContainer drag detection and commit/snapback logic`

---

## Task 6: Implement Widget::draw() for SwipeContainer — swipe background and icon rendering

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/swipe_container.rs` (append to Widget impl)

Implement `draw()`. This renders three layers:
1. **Background colour** — green (right/Done) or orange (left/Snooze), visible behind the displaced child
2. **Action icon** — checkmark (Done) or clock (Snooze), drawn on the exposed background, visible only when armed (past 25% threshold)
3. **Child element** — drawn at horizontal offset (displaced by drag amount)

During SnapBack animation, the offset interpolates from `start_offset` back to 0. During Committing, the offset interpolates from the current position off-screen. During Collapsing, the child is hidden (height is 0 from layout).

```rust
    fn draw(
        &self,
        tree: &iced::advanced::widget::Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: iced::advanced::mouse::Cursor,
        viewport: &Rectangle,
    ) {
        use iced::advanced::renderer::Quad;
        use iced::{Background, Color};

        let bounds = layout.bounds();
        let child_layout = layout.children().next().unwrap();

        // Compute the current visual offset based on the swipe phase.
        let visual_offset = match self.state.phase {
            SwipePhase::Idle => 0.0,
            SwipePhase::Dragging { offset } => offset,
            SwipePhase::SnapBack {
                start_offset,
                started_at,
            } => {
                let elapsed = started_at.elapsed().as_secs_f32();
                let duration = animation::SNAPBACK_DURATION.as_secs_f32();
                let t = (elapsed / duration).min(1.0);
                let eased = animation::ease_out_quad(t);
                start_offset * (1.0 - eased)
            }
            SwipePhase::Committing {
                direction,
                started_at,
            } => {
                let elapsed = started_at.elapsed().as_secs_f32();
                let duration = animation::COMMIT_SLIDE_DURATION.as_secs_f32();
                let t = (elapsed / duration).min(1.0);
                let eased = animation::ease_in_quad(t);
                let target = match direction {
                    SwipeDirection::Right => bounds.width,
                    SwipeDirection::Left => -bounds.width,
                };
                // Start from wherever the commit threshold is (50%) and
                // slide to off-screen.
                let start = match direction {
                    SwipeDirection::Right => self.state.commit_threshold(),
                    SwipeDirection::Left => -self.state.commit_threshold(),
                };
                start + (target - start) * eased
            }
            SwipePhase::Collapsing { .. } | SwipePhase::Done => {
                // Child is hidden during collapse; don't draw.
                return;
            }
        };

        // 1. Draw the swipe background (only if offset != 0).
        if visual_offset.abs() > 0.5 {
            // Determine colour based on direction.
            let (bg_color, icon_color) = if visual_offset > 0.0 {
                // Right swipe = Done = green.
                let armed = visual_offset.abs() >= self.state.arm_threshold();
                let green = if armed {
                    Color::from_rgb8(0x0f, 0x9d, 0x58) // #0f9d58
                } else {
                    Color::from_rgb8(0x34, 0xa8, 0x53) // lighter green pre-arm
                };
                (green, Color::WHITE)
            } else {
                // Left swipe = Snooze = orange.
                let armed = visual_offset.abs() >= self.state.arm_threshold();
                let orange = if armed {
                    Color::from_rgb8(0xef, 0x6c, 0x00) // #ef6c00
                } else {
                    Color::from_rgb8(0xff, 0x9e, 0x40) // lighter orange pre-arm
                };
                (orange, Color::WHITE)
            };

            // Fill the full row bounds with the background colour.
            renderer.fill_quad(
                Quad {
                    bounds: bounds,
                    border: iced::Border::default(),
                    shadow: iced::Shadow::default(),
                },
                Background::Color(bg_color),
            );

            // 2. Draw the action icon (only when armed, past 25%).
            if visual_offset.abs() >= self.state.arm_threshold() {
                // Icon position: centred vertically, 24dp padding from the
                // leading edge of the exposed background.
                // For right swipe: icon is on the left side of the row.
                // For left swipe: icon is on the right side of the row.
                let icon_size = 24.0_f32;
                let icon_padding = 24.0_f32; // bt_swipe_icon_padding from BigTop
                let icon_y = bounds.y + (bounds.height - icon_size) / 2.0;

                let icon_x = if visual_offset > 0.0 {
                    // Right swipe: icon appears on the left, in the green area.
                    bounds.x + icon_padding
                } else {
                    // Left swipe: icon appears on the right, in the orange area.
                    bounds.x + bounds.width - icon_padding - icon_size
                };

                let icon_bounds = Rectangle {
                    x: icon_x,
                    y: icon_y,
                    width: icon_size,
                    height: icon_size,
                };

                // Draw a simple icon representation. In production this would
                // use an icon font or SVG. For now, draw a filled circle as
                // placeholder with the icon colour.
                // TODO(M20): Replace with actual Material icon glyphs
                // (checkmark for Done, clock for Snooze) once the icon system
                // is integrated. For initial implementation, use iced's
                // text rendering with Unicode symbols:
                // Done (right): U+2713 (checkmark)
                // Snooze (left): U+23F0 (alarm clock) or U+25F7 (clock)
                let icon_char = if visual_offset > 0.0 { "\u{2713}" } else { "\u{23F0}" };

                // Render the icon as centred text within icon_bounds.
                renderer.fill_text(
                    iced::advanced::Text {
                        content: icon_char,
                        bounds: Size::new(icon_bounds.width, icon_bounds.height),
                        size: iced::Pixels(icon_size),
                        line_height: iced::advanced::text::LineHeight::Relative(1.0),
                        font: iced::Font::DEFAULT,
                        horizontal_alignment: iced::alignment::Horizontal::Center,
                        vertical_alignment: iced::alignment::Vertical::Center,
                        shaping: iced::advanced::text::Shaping::Basic,
                        wrapping: iced::advanced::text::Wrapping::None,
                    },
                    Point::new(icon_bounds.x, icon_bounds.y),
                    icon_color,
                    icon_bounds,
                );
            }
        }

        // 3. Draw the child element at the visual offset.
        renderer.with_translation(
            Vector::new(visual_offset, 0.0),
            |renderer| {
                self.child.as_widget().draw(
                    &tree.children[0],
                    renderer,
                    theme,
                    style,
                    child_layout,
                    cursor,
                    viewport,
                );
            },
        );
    }
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): implement Widget::draw() for SwipeContainer with background colour and icon rendering`

---

## Task 7: Implement mouse_interaction() and complete the Widget impl

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/swipe_container.rs` (append to Widget impl)

Add `mouse_interaction()` (cursor changes during drag) and the `Into<Element>` conversion. Close the Widget impl block.

```rust
    fn mouse_interaction(
        &self,
        tree: &iced::advanced::widget::Tree,
        layout: Layout<'_>,
        cursor: iced::advanced::mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> iced::mouse::Interaction {
        use iced::mouse::Interaction;

        match self.state.phase {
            SwipePhase::Dragging { .. } => Interaction::Grabbing,
            SwipePhase::Idle => {
                let state = tree.state.downcast_ref::<SwipeDragState>();
                if state.drag_origin.is_some() && !state.drag_started {
                    // Mouse is pressed but drag hasn't started yet.
                    Interaction::Grab
                } else if cursor.is_over(layout.bounds()) {
                    // Hovering — delegate to child for normal cursor.
                    let child_layout = layout.children().next().unwrap();
                    self.child.as_widget().mouse_interaction(
                        &tree.children[0],
                        child_layout,
                        cursor,
                        viewport,
                        renderer,
                    )
                } else {
                    Interaction::default()
                }
            }
            _ => Interaction::default(),
        }
    }

    fn operate(
        &self,
        tree: &mut iced::advanced::widget::Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn iced::advanced::widget::Operation,
    ) {
        let child_layout = layout.children().next().unwrap();
        self.child
            .as_widget()
            .operate(&mut tree.children[0], child_layout, renderer, operation);
    }
}

// --- Into<Element> conversion ---

impl<'a, Message, Theme, Renderer> From<SwipeContainer<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    fn from(container: SwipeContainer<'a, Message, Theme, Renderer>) -> Self {
        Element::new(container)
    }
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): complete Widget impl for SwipeContainer with mouse_interaction and operate`

---

## Task 8: Add animation tick handling to SwipeState

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/swipe_state.rs` (append)

Add a `tick()` method to `SwipeState` that advances animation phases. This is called from the application's `subscription()` or `update()` on every frame tick (using `iced::time::every()`). Returns `true` if the state changed (widget needs redraw) and optionally transitions to the next phase.

```rust
impl SwipeState {
    /// Advance animation state by one tick. Returns `true` if the phase
    /// changed (caller should request a redraw).
    ///
    /// Phase transitions:
    /// - SnapBack → Idle (when animation completes)
    /// - Committing → Collapsing (when slide-off completes)
    /// - Collapsing → Done (when gap collapse completes)
    pub fn tick(&mut self) -> bool {
        match self.phase {
            SwipePhase::SnapBack {
                start_offset,
                started_at,
            } => {
                let elapsed = started_at.elapsed();
                if elapsed >= animation::SNAPBACK_DURATION {
                    self.phase = SwipePhase::Idle;
                    true
                } else {
                    // Still animating — request redraw but no phase change.
                    true
                }
            }
            SwipePhase::Committing {
                direction,
                started_at,
            } => {
                let elapsed = started_at.elapsed();
                if elapsed >= animation::COMMIT_SLIDE_DURATION {
                    // Slide-off complete. Start gap collapse.
                    self.phase = SwipePhase::Collapsing {
                        started_at: Instant::now(),
                        original_height: self.row_width, // Will be set by layout
                    };
                    true
                } else {
                    true
                }
            }
            SwipePhase::Collapsing {
                started_at,
                original_height,
            } => {
                let elapsed = started_at.elapsed();
                if elapsed >= animation::COLLAPSE_DURATION {
                    self.phase = SwipePhase::Done;
                    true
                } else {
                    true
                }
            }
            SwipePhase::Done => false, // Terminal state.
            _ => false,
        }
    }

    /// Reset the state to idle. Used when an action is undone.
    pub fn reset(&mut self) {
        self.phase = SwipePhase::Idle;
        self.hovered = false;
    }

    /// Begin a collapse animation with the given row height.
    /// Called after the commit slide-off completes.
    pub fn begin_collapse(&mut self, height: f32) {
        self.phase = SwipePhase::Collapsing {
            started_at: Instant::now(),
            original_height: height,
        };
    }
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add tick() animation advancement and reset/begin_collapse to SwipeState`

---

## Task 9: Implement hover-reveal action buttons overlay

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/swipe_container.rs` (modify `draw()`)

Add hover action button rendering to the `draw()` method. When `self.state.hovered` is `true` and the swipe phase is `Idle`, draw three icon buttons (Done, Snooze, Pin) as an overlay on the right side of the row. These are the **primary** desktop interaction path.

The buttons are 32x32dp each, spaced 4dp apart, vertically centred, right-aligned with 16dp right padding. They sit on a subtle gradient fade-in from transparent to the card background (so they don't obscure text abruptly).

Insert this block **inside `draw()`**, after the child is drawn and only when `visual_offset == 0.0`:

```rust
        // 4. Draw hover action buttons (only in Idle phase when hovered).
        if self.state.hovered && matches!(self.state.phase, SwipePhase::Idle) {
            let button_size = 32.0_f32;
            let button_spacing = 4.0_f32;
            let right_padding = 16.0_f32;
            let num_buttons = 3;
            let total_buttons_width = (button_size * num_buttons as f32)
                + (button_spacing * (num_buttons - 1) as f32);

            // Gradient fade region (40dp) to the left of the buttons.
            let fade_width = 40.0_f32;
            let buttons_right = bounds.x + bounds.width - right_padding;
            let buttons_left = buttons_right - total_buttons_width;
            let fade_left = buttons_left - fade_width;
            let button_y = bounds.y + (bounds.height - button_size) / 2.0;

            // Draw opaque background behind buttons (card surface colour).
            let surface_color = Color::WHITE; // TODO: use theme.surface()
            renderer.fill_quad(
                Quad {
                    bounds: Rectangle {
                        x: buttons_left,
                        y: bounds.y,
                        width: total_buttons_width + right_padding,
                        height: bounds.height,
                    },
                    border: iced::Border::default(),
                    shadow: iced::Shadow::default(),
                },
                Background::Color(surface_color),
            );

            // Draw each button. Order from right to left: Pin, Snooze, Done.
            let buttons = [
                ("\u{1F4CC}", Color::from_rgb8(0x75, 0x75, 0x75)), // Pin (thumbtack)
                ("\u{23F0}", Color::from_rgb8(0xef, 0x6c, 0x00)),  // Snooze (orange)
                ("\u{2713}", Color::from_rgb8(0x0f, 0x9d, 0x58)),  // Done (green)
            ];

            for (i, (icon, color)) in buttons.iter().enumerate() {
                let btn_x = buttons_right
                    - ((i as f32 + 1.0) * button_size)
                    - (i as f32 * button_spacing);
                let btn_bounds = Rectangle {
                    x: btn_x,
                    y: button_y,
                    width: button_size,
                    height: button_size,
                };

                // Draw circular button background.
                renderer.fill_quad(
                    Quad {
                        bounds: btn_bounds,
                        border: iced::Border {
                            radius: (button_size / 2.0).into(),
                            width: 0.0,
                            color: Color::TRANSPARENT,
                        },
                        shadow: iced::Shadow::default(),
                    },
                    Background::Color(Color::from_rgba8(0, 0, 0, 0.05)),
                );

                // Draw icon.
                renderer.fill_text(
                    iced::advanced::Text {
                        content: icon,
                        bounds: Size::new(btn_bounds.width, btn_bounds.height),
                        size: iced::Pixels(18.0),
                        line_height: iced::advanced::text::LineHeight::Relative(1.0),
                        font: iced::Font::DEFAULT,
                        horizontal_alignment: iced::alignment::Horizontal::Center,
                        vertical_alignment: iced::alignment::Vertical::Center,
                        shaping: iced::advanced::text::Shaping::Basic,
                        wrapping: iced::advanced::text::Wrapping::None,
                    },
                    Point::new(btn_bounds.x, btn_bounds.y),
                    *color,
                    btn_bounds,
                );
            }
        }
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add hover-reveal action buttons (Done/Snooze/Pin) overlay to SwipeContainer`

---

## Task 10: Handle hover button click events in on_event()

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/swipe_container.rs` (modify `on_event()`)

Add hit-testing for the hover action buttons. When the mouse is clicked while hovering over one of the three action buttons, emit the corresponding `SwipeAction` instead of starting a drag. This takes priority over drag initiation.

Insert this block at the top of the `SwipePhase::Idle` arm in `on_event()`, **before** the drag-start logic:

```rust
                // Check hover button clicks first (takes priority over drag).
                if self.state.hovered {
                    if let Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = &event {
                        if let Some(position) = cursor.position() {
                            let button_size = 32.0_f32;
                            let button_spacing = 4.0_f32;
                            let right_padding = 16.0_f32;
                            let buttons_right = bounds.x + bounds.width - right_padding;
                            let button_y = bounds.y + (bounds.height - button_size) / 2.0;

                            // Hit-test each button. Order from right: Pin, Snooze, Done.
                            let actions = [
                                SwipeAction::TogglePin,
                                SwipeAction::Snooze,
                                SwipeAction::Done,
                            ];
                            for (i, action) in actions.iter().enumerate() {
                                let btn_x = buttons_right
                                    - ((i as f32 + 1.0) * button_size)
                                    - (i as f32 * button_spacing);
                                let btn_bounds = Rectangle {
                                    x: btn_x,
                                    y: button_y,
                                    width: button_size,
                                    height: button_size,
                                };
                                if btn_bounds.contains(position) {
                                    shell.publish((self.on_action)(*action));
                                    return Status::Captured;
                                }
                            }
                        }
                    }
                }
```

**Note:** The button positions MUST match exactly between `draw()` and `on_event()`. Extract the position calculations into a helper method `hover_button_bounds(&self, row_bounds: Rectangle) -> [(Rectangle, SwipeAction); 3]` to ensure consistency and avoid duplication. Place this helper on the `SwipeContainer` impl (not the Widget impl).

```rust
impl<'a, Message, Theme, Renderer> SwipeContainer<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    /// Compute the bounds of the three hover action buttons for the given
    /// row bounds. Returns (bounds, action) pairs in right-to-left order:
    /// Pin, Snooze, Done.
    fn hover_button_layout(row_bounds: &Rectangle) -> [(Rectangle, SwipeAction); 3] {
        let button_size = 32.0_f32;
        let button_spacing = 4.0_f32;
        let right_padding = 16.0_f32;
        let buttons_right = row_bounds.x + row_bounds.width - right_padding;
        let button_y = row_bounds.y + (row_bounds.height - button_size) / 2.0;

        let actions = [
            SwipeAction::TogglePin,
            SwipeAction::Snooze,
            SwipeAction::Done,
        ];

        let mut result = [(Rectangle::default(), SwipeAction::Done); 3];
        for (i, action) in actions.iter().enumerate() {
            let btn_x = buttons_right
                - ((i as f32 + 1.0) * button_size)
                - (i as f32 * button_spacing);
            result[i] = (
                Rectangle {
                    x: btn_x,
                    y: button_y,
                    width: button_size,
                    height: button_size,
                },
                *action,
            );
        }
        result
    }
}
```

Then use `Self::hover_button_layout(&bounds)` in both `draw()` and `on_event()`.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): handle hover action button clicks with shared hit-test layout`

---

## Task 11: Add hover state tracking via cursor enter/leave events

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/swipe_container.rs` (modify `on_event()`)

Implement proper hover tracking. In Iced, there is no explicit "mouse enter" / "mouse leave" event — hover must be detected by checking `cursor.is_over(bounds)` on every `CursorMoved` event. The SwipeContainer needs to emit a message when hover state changes so the app can update `SwipeState.hovered`.

Add a dedicated message type for hover changes. Since the SwipeContainer already has `on_state_change` for phase changes, add a separate `on_hover` callback:

```rust
// Add to SwipeContainer struct:
    /// Callback when hover state changes. Receives `true` for enter, `false` for leave.
    on_hover: Option<Box<dyn Fn(bool) -> Message + 'a>>,
```

Update the constructor and add a builder method:

```rust
    /// Set a callback for hover state changes.
    pub fn on_hover(mut self, f: impl Fn(bool) -> Message + 'a) -> Self {
        self.on_hover = Some(Box::new(f));
        self
    }
```

In `on_event()`, at the top (before any phase-specific logic), add:

```rust
        // Track hover state changes.
        if let Event::Mouse(mouse::Event::CursorMoved { .. }) = &event {
            let is_over = cursor.is_over(bounds);
            if is_over != self.state.hovered {
                if let Some(on_hover) = &self.on_hover {
                    shell.publish(on_hover(is_over));
                }
            }
        }
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add on_hover callback for hover state tracking in SwipeContainer`

---

## Task 12: Add animation subscription for active swipe states

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/swipe_state.rs` (append)

The application needs to drive animations by subscribing to frame ticks when any SwipeState has an active animation. Add a helper that checks whether a tick subscription is needed.

```rust
impl SwipeState {
    /// Returns `true` if this state has an active animation that needs
    /// frame-by-frame tick updates.
    pub fn needs_tick(&self) -> bool {
        matches!(
            self.phase,
            SwipePhase::SnapBack { .. }
                | SwipePhase::Committing { .. }
                | SwipePhase::Collapsing { .. }
        )
    }
}
```

**Also:** Document how the application should wire this up in its `subscription()`:

```rust
// In the application's subscription() method:
//
//   fn subscription(&self) -> Subscription<Message> {
//       // Check if any row has an active animation.
//       let any_animating = self.swipe_states.values().any(|s| s.needs_tick());
//       if any_animating {
//           iced::time::every(Duration::from_millis(16))
//               .map(|_| Message::AnimationTick)
//       } else {
//           Subscription::none()
//       }
//   }
//
// In update() for Message::AnimationTick:
//
//   for state in self.swipe_states.values_mut() {
//       state.tick();
//   }
//   // Remove rows where state.phase == SwipePhase::Done
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): add needs_tick() helper for animation subscription gating`

---

## Task 13: Wire SwipeContainer into the inbox feed view

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/views/inbox_feed.rs` (modify)

Update the inbox feed view to wrap each `EmailRow` in a `SwipeContainer`. This requires:

1. Add a `HashMap<ThreadId, SwipeState>` (or equivalent keyed map) to the application model
2. In the inbox feed's `view()` method, wrap each email row:

```rust
// Before (M19):
//   EmailRow::new(&thread, &theme).into()
//
// After (M20):
//   let swipe_state = self.swipe_states
//       .entry(thread.id.clone())
//       .or_default();
//
//   SwipeContainer::new(
//       EmailRow::new(&thread, &theme),
//       swipe_state,
//       move |action| match action {
//           SwipeAction::Done => Message::MarkDone(thread_id.clone()),
//           SwipeAction::Snooze => Message::OpenSnoozePicker(thread_id.clone()),
//           SwipeAction::TogglePin => Message::TogglePin(thread_id.clone()),
//       },
//       move |phase| Message::SwipePhaseChanged(thread_id.clone(), phase),
//   )
//   .on_hover(move |hovered| Message::RowHovered(thread_id.clone(), hovered))
//   .into()
```

3. Add `Message::SwipePhaseChanged(ThreadId, SwipePhase)`, `Message::RowHovered(ThreadId, bool)`, and `Message::AnimationTick` to the application `Message` enum.

4. Handle these messages in `update()`:

```rust
Message::SwipePhaseChanged(id, phase) => {
    if let Some(state) = self.swipe_states.get_mut(&id) {
        state.phase = phase;
    }
}
Message::RowHovered(id, hovered) => {
    if let Some(state) = self.swipe_states.get_mut(&id) {
        state.hovered = hovered;
    }
}
Message::AnimationTick => {
    let mut done_ids = Vec::new();
    for (id, state) in &mut self.swipe_states {
        state.tick();
        if matches!(state.phase, SwipePhase::Done) {
            done_ids.push(id.clone());
        }
    }
    for id in done_ids {
        self.swipe_states.remove(&id);
        // Row removal from the feed is handled by the Done/Snooze
        // action that was already dispatched in Task 5.
    }
}
```

5. Add the animation subscription in `subscription()`:

```rust
fn subscription(&self) -> Subscription<Message> {
    let any_animating = self.swipe_states.values().any(|s| s.needs_tick());
    if any_animating {
        iced::time::every(std::time::Duration::from_millis(16))
            .map(|_| Message::AnimationTick)
    } else {
        Subscription::none()
    }
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `feat(ui): wire SwipeContainer into inbox feed with state management and animation subscription`

---

## Task 14: Store row_width in SwipeState during layout

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/swipe_container.rs` (modify `layout()`)

The `SwipeState.row_width` field must be updated each time the widget is laid out, so that threshold calculations (25% and 50%) use the correct value. Since the Widget trait's `layout()` receives an immutable `&self`, we cannot mutate `self.state` directly. Instead, emit a one-time message when the width changes, or store the width in the internal `SwipeDragState` (widget tree state) and pass it to threshold calculations.

The cleanest approach: store `row_width` in `SwipeDragState` (tree state) and use it in `on_event()` for threshold checks, since `on_event()` receives `&mut self` for the tree.

Update `SwipeDragState`:

```rust
#[derive(Debug, Default)]
struct SwipeDragState {
    drag_origin: Option<f32>,
    drag_started: bool,
    /// Cached row width from the most recent layout pass.
    last_row_width: f32,
}
```

In `on_event()`, at the top, cache the layout width:

```rust
        // Cache the row width for threshold calculations.
        let state = tree.state.downcast_mut::<SwipeDragState>();
        state.last_row_width = bounds.width;
```

Then use `state.last_row_width` instead of `self.state.row_width` for threshold calculations in `on_event()`. The `SwipeState.row_width` in the app model is still useful for the `draw()` method (which uses `self.state`), so also emit a width-update message if the width changed:

```rust
        if (self.state.row_width - bounds.width).abs() > 0.5 {
            // Emit a message so the app can update SwipeState.row_width.
            // This is needed for draw() which uses self.state.
            // The on_state_change callback is reused with the current phase
            // (no phase change, just a width update trigger).
        }
```

Alternatively, compute thresholds from `bounds.width` directly in both `draw()` and `on_event()`, eliminating the need for `row_width` in `SwipeState` entirely. This is the simpler approach. **Recommended: remove `row_width` from `SwipeState` and compute thresholds from layout bounds directly.**

Update `SwipeState` to take `row_width` as a parameter:

```rust
    pub fn arm_threshold_for(row_width: f32) -> f32 {
        row_width * 0.25
    }

    pub fn commit_threshold_for(row_width: f32) -> f32 {
        row_width * 0.50
    }
```

And update `draw()` and `on_event()` to call `SwipeState::arm_threshold_for(bounds.width)` instead of `self.state.arm_threshold()`.

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui
```

**Commit:** `refactor(ui): compute swipe thresholds from layout bounds directly, remove row_width from SwipeState`

---

## Task 15: Add unit tests for SwipeState phase transitions

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/swipe_state.rs` (append)

Add comprehensive tests for the state machine transitions.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn default_state_is_idle() {
        let state = SwipeState::default();
        assert!(matches!(state.phase, SwipePhase::Idle));
        assert!(!state.hovered);
    }

    #[test]
    fn arm_threshold_is_25_percent() {
        assert!((SwipeState::arm_threshold_for(400.0) - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn commit_threshold_is_50_percent() {
        assert!((SwipeState::commit_threshold_for(400.0) - 200.0).abs() < f32::EPSILON);
    }

    #[test]
    fn snapback_completes_to_idle() {
        let mut state = SwipeState::default();
        state.phase = SwipePhase::SnapBack {
            start_offset: 50.0,
            started_at: Instant::now() - animation::SNAPBACK_DURATION - Duration::from_millis(10),
        };
        assert!(state.tick());
        assert!(matches!(state.phase, SwipePhase::Idle));
    }

    #[test]
    fn commit_transitions_to_collapsing() {
        let mut state = SwipeState::default();
        state.phase = SwipePhase::Committing {
            direction: SwipeDirection::Right,
            started_at: Instant::now()
                - animation::COMMIT_SLIDE_DURATION
                - Duration::from_millis(10),
        };
        assert!(state.tick());
        assert!(matches!(state.phase, SwipePhase::Collapsing { .. }));
    }

    #[test]
    fn collapse_transitions_to_done() {
        let mut state = SwipeState::default();
        state.phase = SwipePhase::Collapsing {
            started_at: Instant::now()
                - animation::COLLAPSE_DURATION
                - Duration::from_millis(10),
            original_height: 72.0,
        };
        assert!(state.tick());
        assert!(matches!(state.phase, SwipePhase::Done));
    }

    #[test]
    fn done_is_terminal() {
        let mut state = SwipeState::default();
        state.phase = SwipePhase::Done;
        assert!(!state.tick());
    }

    #[test]
    fn idle_does_not_need_tick() {
        let state = SwipeState::default();
        assert!(!state.needs_tick());
    }

    #[test]
    fn snapback_needs_tick() {
        let mut state = SwipeState::default();
        state.phase = SwipePhase::SnapBack {
            start_offset: 100.0,
            started_at: Instant::now(),
        };
        assert!(state.needs_tick());
    }

    #[test]
    fn reset_returns_to_idle() {
        let mut state = SwipeState::default();
        state.phase = SwipePhase::Dragging { offset: 150.0 };
        state.hovered = true;
        state.reset();
        assert!(matches!(state.phase, SwipePhase::Idle));
        assert!(!state.hovered);
    }

    #[test]
    fn easing_functions_boundary_values() {
        assert!((animation::ease_out_quad(0.0)).abs() < f32::EPSILON);
        assert!((animation::ease_out_quad(1.0) - 1.0).abs() < f32::EPSILON);
        assert!((animation::ease_in_quad(0.0)).abs() < f32::EPSILON);
        assert!((animation::ease_in_quad(1.0) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn easing_is_monotonic() {
        for i in 0..10 {
            let t1 = i as f32 / 10.0;
            let t2 = (i + 1) as f32 / 10.0;
            assert!(animation::ease_out_quad(t2) >= animation::ease_out_quad(t1));
            assert!(animation::ease_in_quad(t2) >= animation::ease_in_quad(t1));
        }
    }
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui -- swipe_state
```

**Commit:** `test(ui): add unit tests for SwipeState phase transitions and animation easing`

---

## Task 16: Add integration test verifying swipe threshold behaviour

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/tests/swipe_integration.rs` (new file)

Test the full swipe flow from drag start to action emission without rendering. This validates the logical state machine independently of Iced's rendering pipeline.

```rust
//! Integration tests for SwipeContainer's swipe-to-action logic.

use inboxly_ui::widgets::swipe_state::*;
use std::time::{Duration, Instant};

/// Simulate a right swipe past commit threshold.
#[test]
fn right_swipe_past_commit_triggers_done() {
    let row_width = 400.0;
    let mut state = SwipeState::default();

    // Phase 1: Start dragging right.
    state.phase = SwipePhase::Dragging { offset: 10.0 };

    // Phase 2: Cross arm threshold (25% = 100px).
    state.phase = SwipePhase::Dragging { offset: 120.0 };
    // At this point, the icon should be visible.

    // Phase 3: Cross commit threshold (50% = 200px).
    state.phase = SwipePhase::Dragging { offset: 210.0 };
    assert!(210.0_f32.abs() >= SwipeState::commit_threshold_for(row_width));

    // Phase 4: Release → should transition to Committing (right).
    state.phase = SwipePhase::Committing {
        direction: SwipeDirection::Right,
        started_at: Instant::now() - Duration::from_millis(200),
    };
    state.tick();
    assert!(matches!(state.phase, SwipePhase::Collapsing { .. }));
}

/// Simulate a left swipe that reverses before commit → snapback.
#[test]
fn left_swipe_reversed_snaps_back() {
    let mut state = SwipeState::default();

    // Drag left past arm but not past commit.
    state.phase = SwipePhase::Dragging { offset: -120.0 };

    // Release: offset is below commit threshold (200px) → snapback.
    assert!(120.0_f32.abs() < SwipeState::commit_threshold_for(400.0));

    state.phase = SwipePhase::SnapBack {
        start_offset: -120.0,
        started_at: Instant::now() - Duration::from_millis(300),
    };
    state.tick();
    assert!(matches!(state.phase, SwipePhase::Idle));
}

/// Full lifecycle: drag → commit → slide → collapse → done.
#[test]
fn full_swipe_lifecycle() {
    let mut state = SwipeState::default();

    // Idle → Dragging.
    state.phase = SwipePhase::Dragging { offset: 250.0 };

    // Dragging → Committing.
    state.phase = SwipePhase::Committing {
        direction: SwipeDirection::Right,
        started_at: Instant::now() - animation::COMMIT_SLIDE_DURATION - Duration::from_millis(1),
    };

    // Committing → Collapsing.
    state.tick();
    assert!(matches!(state.phase, SwipePhase::Collapsing { .. }));

    // Fast-forward collapse.
    if let SwipePhase::Collapsing { original_height, .. } = state.phase {
        state.phase = SwipePhase::Collapsing {
            started_at: Instant::now() - animation::COLLAPSE_DURATION - Duration::from_millis(1),
            original_height,
        };
    }

    // Collapsing → Done.
    state.tick();
    assert!(matches!(state.phase, SwipePhase::Done));

    // Done is terminal.
    assert!(!state.tick());
}
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo test -p inboxly-ui -- swipe_integration
```

**Commit:** `test(ui): add integration tests for full swipe lifecycle and snapback`

---

## Task 17: Export SwipeContainer and SwipeState from inboxly-ui public API

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/widgets/mod.rs` (verify)

Ensure the public API surface is clean:

```rust
pub mod swipe_container;
pub mod swipe_state;

pub use swipe_container::SwipeContainer;
pub use swipe_state::{SwipeAction, SwipeDirection, SwipePhase, SwipeState};
```

**File:** `/mnt/TempNVME/projects/inbox-rust/inboxly-ui/src/lib.rs` (verify)

```rust
pub mod widgets;
```

This allows downstream code (the inbox feed view and the binary) to import:

```rust
use inboxly_ui::widgets::{SwipeContainer, SwipeState, SwipeAction, SwipePhase};
```

**Verify:**

```bash
cd /mnt/TempNVME/projects/inbox-rust && cargo check -p inboxly-ui && cargo doc -p inboxly-ui --no-deps
```

**Commit:** `feat(ui): export SwipeContainer and SwipeState from inboxly-ui public API`

---

## Summary

| Task | What | File(s) | Test |
|------|------|---------|------|
| 1 | SwipePhase, HoverState, SwipeState types | `widgets/swipe_state.rs` | `cargo check` |
| 2 | SwipeAction enum + animation constants | `widgets/swipe_state.rs` | `cargo check` |
| 3 | SwipeContainer struct + constructor | `widgets/swipe_container.rs` | `cargo check` |
| 4 | Widget::layout() with collapse animation | `widgets/swipe_container.rs` | `cargo check` |
| 5 | Widget::on_event() drag detection + commit/snapback | `widgets/swipe_container.rs` | `cargo check` |
| 6 | Widget::draw() background + icon rendering | `widgets/swipe_container.rs` | `cargo check` |
| 7 | mouse_interaction() + Into<Element> | `widgets/swipe_container.rs` | `cargo check` |
| 8 | SwipeState::tick() animation advancement | `widgets/swipe_state.rs` | `cargo check` |
| 9 | Hover action buttons overlay in draw() | `widgets/swipe_container.rs` | `cargo check` |
| 10 | Hover button click handling in on_event() | `widgets/swipe_container.rs` | `cargo check` |
| 11 | Hover state tracking via on_hover callback | `widgets/swipe_container.rs` | `cargo check` |
| 12 | Animation subscription helper | `widgets/swipe_state.rs` | `cargo check` |
| 13 | Wire into inbox feed view | `views/inbox_feed.rs` | `cargo check` |
| 14 | Threshold from layout bounds refactor | `widgets/swipe_container.rs`, `swipe_state.rs` | `cargo check` |
| 15 | Unit tests for SwipeState | `widgets/swipe_state.rs` | `cargo test` |
| 16 | Integration tests for swipe lifecycle | `tests/swipe_integration.rs` | `cargo test` |
| 17 | Public API exports | `widgets/mod.rs`, `lib.rs` | `cargo doc` |

### Iced Widget Implementation Notes

- **Elm architecture constraint:** The widget cannot mutate application state. All state changes are communicated via messages (`on_action`, `on_state_change`, `on_hover` callbacks). The application's `update()` method applies these to the `SwipeState` stored in the model.
- **Widget tree state:** Per-instance mutable state (drag origin position) uses Iced's `widget::Tree` state mechanism (`tag()`, `state()`, `downcast_mut()`). This is separate from `SwipeState` — tree state is internal to the widget, while `SwipeState` is in the app model.
- **Animation:** Iced has no built-in animation system. Animations are driven by `iced::time::every()` subscription that emits `AnimationTick` messages at ~60fps. Each tick calls `SwipeState::tick()` which checks elapsed time against duration constants. The subscription is gated by `needs_tick()` so it only runs when animations are active.
- **Renderer API:** `fill_quad()` for solid rectangles, `fill_text()` for icon glyphs, `with_translation()` for offsetting the child during swipe. All are on `iced::advanced::Renderer`.
- **Icon rendering:** Initial implementation uses Unicode symbols (checkmark, clock, thumbtack). A follow-up task should integrate Material Icons font or SVG icons for pixel-perfect rendering.
