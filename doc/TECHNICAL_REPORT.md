# Technical Report: iced-liveplot

## Overview

`iced-liveplot` is a high-performance, real-time plotting widget for the Iced GUI framework. It is designed for streaming data visualization with sub-millisecond render times, supporting millions of points through adaptive decimation.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Application Layer                        │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │   Plot      │  │   Series    │  │   Configuration         │  │
│  │   Builder   │  │   Factory   │  │   (Axis, Theme, View)   │  │
│  └─────────────┘  └─────────────┘  └─────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                     Widget Layer (Canvas)                       │
│  ┌─────────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │   Program       │  │   State     │  │   Events            │  │
│  │   (Trait)       │  │   (UI)      │  │   (Interaction)     │  │
│  └─────────────────┘  └─────────────┘  └─────────────────────┘  │
│         │                   │                   │                │
│         ▼                   ▼                   ▼                │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │                    Frame Builder                         │    │
│  │  (Grid → Series → Pins → Axes → Hover → Linked → Legend) │    │
│  └─────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Data Layer                                 │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │  AppendOnly │  │  Series     │  │  Decimation             │  │
│  │  Data       │  │  Store      │  │  (MinMax Summary)       │  │
│  └─────────────┘  └─────────────┘  └─────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Render Layer                               │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │  Commands   │  │  Geometry   │  │  Canvas Drawing         │  │
│  │  (Line,     │  │  (Transform,│  │  (Iced Canvas)          │  │
│  │   Scatter)  │  │   Clipping) │  │                         │  │
│  └─────────────┘  └─────────────┘  └─────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

## Core Modules

### 1. Data Layer (`src/datasource/`)

#### `AppendOnlyData`
- **Purpose**: Lock-free append-only storage optimized for streaming
- **Modes**: `Index` (implicit X = 0,1,2...) and `Explicit` (user-provided X)
- **Features**: 
  - Incremental bounds tracking
  - Monotonicity detection for binary search optimization
  - O(1) append, O(log n) range queries (when monotonic)

#### `SeriesStore`
- Wraps `AppendOnlyData` with decimation support
- Generations for cache invalidation
- Base chunk size configurable (default 64)

#### `Summary` / `DecimationScratch`
- Hierarchical MinMax decimation (LTTB-inspired)
- Multiple resolution levels for zoom-independent rendering
- Scratch buffer for allocation-free decimation

### 2. Geometry & Transform (`src/geom.rs`, `src/transform.rs`)

- `Point` / `ScreenPoint`: Data vs screen coordinates
- `ScreenRect`: Pixel rectangles
- `Transform`: Viewport ↔ Screen mapping with clipping awareness

### 3. View System (`src/view.rs`)

- `Range`: Min/max with padding, clamping, union operations
- `Viewport`: X+Y ranges with validation

### 4. Axis System (`src/axis.rs`)

- `AxisConfig`: Declarative axis configuration (builder pattern)
- `TickConfig`: Linear/Log/Time tick generation
- `AxisFormatter`: Custom formatting closures
- Automatic tick density based on pixel length

### 5. Series (`src/series.rs`)

- `Series`: Thread-safe handle with `Arc<RwLock<SeriesStore>>`
- `SeriesKind`: Line or Scatter with styling
- `Series::share()`: Multiple handles, same data (live streaming)
- `Series::clone()`: Independent copy

### 6. Plot (`src/plot.rs`)

- `Plot`: Container for series, axes, pins, viewport
- `PlotBuilder`: Fluent construction
- Manual viewport control + auto-fit

### 7. Widget (`src/widget/`)

#### `mod.rs` - `Program` Trait
```rust
pub trait Program {
    fn plot(&self) -> Ref<Plot>;
    fn with_plot_mut<R>(&self, f: impl FnOnce(&mut Plot) -> R) -> R;
    fn config(&self) -> &PlotViewConfig;
    fn theme(&self) -> Theme;
    // ... link binding, redraw requests
}
```

#### `state.rs` - `PlotUiState`
All transient UI state:
- Layout caches (axis, plot rect, transform)
- Drag state (pan, zoom rect, zoom wheel)
- Hover target, selection rect
- **NEW**: `last_click` + `last_pin_toggle` for double-click handling
- Linked cursor/brush state

#### `events.rs` - Interaction Handling
```rust
pub(super) fn handle(
    program: &Program,
    state: &mut PlotUiState,
    event: &Event,
    bounds: Rectangle,
    cursor: mouse::Cursor,
) -> Option<canvas::Action<Message>>
```

**Event Flow**:
```
MouseDown → handle_mouse_down
  ├─ Left: Start pan drag OR click detection
  ├─ Right: Start zoom-rect drag
  └─ Middle: Start pan drag

MouseMove → handle_mouse_move
  ├─ Drag active: Update pan/zoom rect
  └─ No drag: Update hover target

MouseUp → handle_mouse_up
  ├─ Drag commit (pan/zoom-rect)
  ├─ Click: Execute pin toggle IMMEDIATELY
  │   └─ Store in last_pin_toggle for potential undo
  └─ Double-click (<300ms, <6px): UNDO pin + emit ResetView

Wheel → handle_scroll
  └─ Zoom at cursor (Ctrl+Wheel = horizontal)
```

#### `hover.rs` - Hit Testing
- Pinned points: exact match within threshold
- Unpinned: nearest point within threshold
- Series-aware (uses SeriesId + index)

#### `frame.rs` - Render Pipeline
```
build_frame()
  ├─ build_grid()
  ├─ build_series() → decimate → RenderCommands
  ├─ build_pins() → clustered labels
  ├─ build_axes() → ticks + labels
  ├─ build_hover() → tooltip
  ├─ build_linked_cursor()
  ├─ build_linked_brush()
  └─ build_legend()
```

#### `paint.rs` - Canvas Drawing
- `RenderCommand`: LineSegments, ScatterPoints, Rect, Text
- Clipping to plot rect
- Stroke/Fill style application

### 8. Interactions (`src/interaction.rs`)

Pure functions for viewport manipulation:
- `hit_test()`: Region detection (Plot, Axis, Legend, etc.)
- `toggle_pin()`: Add/remove pin from Vec
- `pan_viewport()`: Pixel delta → data delta
- `zoom_viewport()`: Center + factor
- `zoom_to_rect()`: Screen rect → new viewport
- `zoom_factor_from_drag()`: Wheel/gesture zoom factor

### 9. Linking (`src/widget/link.rs`)

Multi-plot synchronization:
- `PlotLinkGroup`: Shared synchronization group
- `PlotLinkOptions`: View / Cursor X / Brush X
- Message-based propagation (no direct mutation)

---

## Performance Characteristics

| Operation | Complexity | Notes |
|-----------|------------|-------|
| Append point | O(1) amortized | Vec push + bounds update |
| Range query | O(log n) | Binary search (monotonic) |
| Decimation | O(n) | Single pass, allocation-free |
| Render | O(chunks) | Fixed ~1000 segments max |
| Hit test | O(series × log n) | Pinned first, then nearest |

---

## Threading Model

```
Main Thread (UI)          Background Threads (Data)
┌─────────────────┐       ┌─────────────────────┐
│ Iced Event Loop │       │ Data Acquisition    │
│   - handle()    │◄──────│   - push_y()        │
│   - draw()      │       │   - extend_y()      │
│                 │       │   - push_point()    │
│ PlotUiState     │       │   (Arc<RwLock>)     │
│ (mut, !Send)    │       │                     │
│ Program trait   │       │ Series::share()     │
│ (RefCell)       │       │ (new SeriesId)      │
└─────────────────┘       └─────────────────────┘
```

- `Series` uses `Arc<RwLock>` for concurrent append + read
- UI state is `!Send` (main thread only)
- Generations enable lock-free render cache invalidation

---

## Key Design Decisions

1. **Append-only data**: Enables streaming, decimation, lock-free reads
2. **Decimation at render time**: Zoom-level appropriate resolution
3. **Immediate pin execution + undo**: Responsive UX, no ghost pins
4. **Program trait**: Decouples widget from app logic
5. **Link groups**: Composable multi-plot synchronization
6. **Builder patterns**: Ergonomic configuration