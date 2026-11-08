# iced-liveplot Implementation Summary

## Overview

This document provides a comprehensive technical summary of the iced-liveplot library implementation, covering the architecture, core modules, interaction system, and future extensibility points. The library is a port of `gpui-liveplot` to the `iced` GUI framework (version 0.15-dev from git master).

## Architecture

### Core Components

```
iced-liveplot/
├── src/
│   ├── lib.rs              # Main library entry point, public exports
│   ├── plot.rs             # Plot model, builder, viewport management
│   ├── series.rs           # Data series with append-only storage
│   ├── interaction.rs      # Platform-agnostic interaction helpers
│   ├── geom.rs             # Geometry primitives (Point, Rect, Transform)
│   ├── transform.rs        # Data-to-screen coordinate transforms
│   ├── view.rs             # Viewport, ranges, view management
│   ├── render/             # Rendering pipeline (commands, cache, decimation)
│   ├── datasource/         # Data storage with decimation support
│   ├── style.rs            # Theming (colors, fonts, spacing)
│   ├── axis.rs             # Axis layout and rendering
│   ├── region.rs           # Domain-agnostic region hierarchy
│   ├── selection.rs        # Multi-selection model
│   ├── snap.rs             # Snap/grid engine
│   ├── midi/               # MIDI domain (piano roll)
│   ├── audio/              # Audio domain (waveforms, clips)
│   ├── automation/         # Automation domain (parameter curves)
│   └── widget/             # Iced canvas backend
│       ├── mod.rs          # Program trait, message types
│       ├── actions.rs      # Action/Command framework
│       ├── dispatcher.rs   # Action dispatcher with history
│       ├── history.rs      # Undo/Redo manager
│       ├── keybindings.rs  # Keybinding system
│       ├── tools.rs        # Tool system (Select, Pen, Eraser, etc.)
│       ├── frame.rs        # Frame building & rendering
│       ├── events.rs       # Event handling (mouse, scroll, keyboard)
│       ├── state.rs        # UI state (hover, drag, pins, regions)
│       ├── hover.rs        # Hover target computation
│       ├── link.rs         # Multi-plot synchronization
│       └── ...             # Geometry, paint, text, config modules
```

### Data Flow

```
User Input (Mouse/Keyboard)
    │
    ▼
widget/events.rs (handle())
    │
    ├──▶ Mouse Events ──▶ handle_mouse_down/move/up
    ├──▶ Scroll Events ──▶ handle_scroll
    └──▶ Keyboard Events ──▶ Keybindings lookup
    │
    ▼
Program.update()
    │
    ├──▶ Actions → Dispatcher → History (undo/redo)
    ├──▶ Tools → Tool handlers
    ├──▶ Selection updates
    └──▶ Viewport changes
    │
    ▼
widget/frame.rs (build_frame())
    │
    ├──▶ build_grid()
    ├──▶ build_series() (with decimation)
    ├──▶ build_regions() (NEW: domain-agnostic regions)
    ├──▶ build_selection()
    ├──▶ build_linked_cursor/brush()
    └──▶ update_hover_target()
    │
    ▼
RenderList → canvas::Program::draw()
```

## Key Modules

### 1. Action/Command Framework (`widget/actions.rs`, `widget/dispatcher.rs`)

Provides a robust action system with full undo/redo support:

- **`PlotAction` trait**: Extensible action interface with `execute()`, `undo()`, `can_undo()`
- **`ActionContext`**: Provides access to plot, state, config, clipboard during execution
- **`ActionResult`**: Success, Redraw, Publish(message), or Error variants
- **`ActionDispatcher`**: Executes actions, manages history groups with grouping window
- **`HistoryGroup`**: Batches related actions (e.g., drag operations)
- **`Clipboard`**: Cross-action data transfer (copy/paste/cut)

**Built-in Actions:**
- Pin CRUD: `AddPin`, `RemovePin`, `TogglePin`, `MovePin`
- View: `SetViewport`, `ResetView`, `ZoomToRect`
- Selection: `SelectAll`, `ClearSelection`, `ExtendSelection`
- Region: `AddRegion`, `RemoveRegion`, `MoveRegion`, `ResizeRegion`
- Clipboard: `CopySelection`, `CutSelection`, `PasteClipboard`
- Batch: `BatchAction` for grouping multiple actions

### 2. Keybindings System (`widget/keybindings.rs`)

Configurable keyboard shortcuts for standard actions:

- **`StandardAction` enum**: 40+ predefined actions (Undo, Redo, Copy, Cut, Paste, Delete, Zoom, Pan, Tool switching, etc.)
- **`Chord`**: Key + modifiers (Ctrl, Shift, Alt, combinations)
- **`KeyBindings`**: Maps keys/chords to actions with `bind()`, `bind_chord()`, `unbind()`
- **Default bindings**: Ctrl+Z/Cmd+Z (Undo), Ctrl+Y/Cmd+Shift+Z (Redo), Ctrl+C/V/X (Copy/Paste/Cut), etc.

### 3. History/Undo-Redo Manager (`widget/history.rs`)

- **`ActionDispatcher`** core with undo/redo stacks
- **Action grouping**: Consecutive actions within time window grouped (default 500ms)
- **Configurable limits**: Max history size, group window duration
- **Change callbacks**: UI notifications on history changes

### 4. Region System (`region.rs`)

Domain-agnostic hierarchical region model:

```rust
pub enum RegionKind {
    Pin(Pin),                    // Single point marker
    Range(Range),                // Time range [start, end]
    Rectangle(Rect),             // 2D rectangle
    MidiNote { pitch, velocity, channel, ... },
    AudioRegion { source, gain, fade_in, fade_out, ... },
    AutomationPoint { parameter_id, curve_type },
    AutomationCurve { points: Vec<AutomationPoint> },
    Marker { label, color },
    Loop { start, end },
    Custom(String, HashMap<String, Value>),
}
```

**Key features:**
- `RegionId` with atomic counter for unique IDs
- `RegionStyle` (fill, stroke, stroke_width, label_color, label_position)
- `DataRegion` wrapper with visibility, locking, selection, z-index, metadata
- `RegionRegistry` for type registration and factory methods
- Screen bounds computation via transform

### 5. Selection Model (`selection.rs`)

Multi-selection support with keyboard modifiers:

- **`Selection`**: Tracks selected pins, regions, points per series
- **`SelectionAnchor`**: Anchor point for shift-extend operations
- **`Selectable` trait**: For custom selectable types
- **Keyboard modifiers**: Shift (extend), Ctrl/Cmd (toggle), none (replace)

### 6. Snap/Grid System (`snap.rs`)

Configurable snapping for precise editing:

- **`TimeBase`**: Samples, Seconds, Milliseconds, Frames, Beats, Bars, Ticks
- **`GridSpec`**: Division (16th notes default), swing, offset
- **`SnapConfig`**: Enabled, grid spec, threshold, multi-grid support
- **`SnapEngine`**: `snap_point()`, `snap_to_grid()`, `snap_to_nearest()`
- **`SnapResult`**: Snapped value, grid line index, distance

### 7. Tool System (`widget/tools.rs`)

Mode-based editing tools:

```rust
pub enum Tool {
    Select,    // Click select, drag box-select
    Pen,       // Add points/regions
    Eraser,    // Delete points/regions
    Move,      // Drag to move selection
    Scale,     // Drag to scale
    Stretch,   // Time-stretch
    Split,     // Split region at cursor
    Join,      // Join adjacent regions
    Draw,      // Freehand automation
    Line,      // Straight line automation
    Curve,     // Bezier curve automation
    Velocity,  // Edit MIDI velocities
    Quantize,  // Snap to grid
}
```

- **`ToolHandler` trait**: Custom tool behavior
- **`ToolRegistry`**: Manages available tools and handlers
- **`ToolState`**: Current tool, active tool data, cursor management

### 8. Domain Modules

#### MIDI (`midi/mod.rs`)
- `MidiNote`: pitch, velocity, start, duration, channel
- `MidiSequence`: Collection of notes with quantization
- `MidiTrack`: Multi-channel sequence with piano roll config
- `PianoRollRenderer`: Draw notes as rectangles with velocity bars
- `DrumMap`: Note name mapping for percussion

#### Audio (`audio/mod.rs`)
- `AudioRegion`: Source (buffer/generator), gain, fades, pitch, stretch
- `AudioClip`: Reference to region with timeline position
- `WaveformRenderer`: Decimated waveform drawing with fade visualization
- `AudioRegionRenderer`: Region envelope + waveform overview

#### Automation (`automation/mod.rs`)
- `AutomationPoint`: time, value, curve_type
- `CurveType`: Linear, Exponential, Logarithmic, Sine, Hold, Bezier
- `AutomationLane`: Parameter ID, range, points, style
- `AutomationTrack`: Multiple lanes with solo/mute
- `AutomationRenderer`: Curve interpolation and drawing

## Interaction System (`widget/events.rs`)

### Event Handling Architecture

The event system uses **manual double-click detection** (not `mouse_area` wrapper) to fix two critical bugs:

1. **Ghost Pin Bug**: Double-click to ResetView created a pin from first click
2. **One-Click-Behind Bug**: Single-click pin insertion lagged by one click

### Solution: Immediate Execution + Undo Pattern

```rust
// On first click: EXECUTE immediately, store for potential undo
execute_pin_and_store(program, state, pos, button);

// On second click (within 300ms, 6px): UNDO first pin, emit ResetView
if is_double_click(&last_click, pos, button) {
    if let Some(pin_toggle) = state.last_pin_toggle.take() {
        // Undo the pin from first click
        if pin_toggle.added { pins.remove(pin) } else { pins.push(pin) }
    }
    emit(ResetView);
}
```

**Thresholds:**
- `DOUBLE_CLICK_THRESHOLD_MS = 300`
- `DOUBLE_CLICK_THRESHOLD_PX = 6.0`

### Event Types Handled

- `Mouse::ButtonPressed` → `handle_mouse_down()`
- `Mouse::CursorMoved` → `handle_mouse_move()`
- `Mouse::ButtonReleased` → `handle_mouse_up()` (with double-click logic)
- `Mouse::WheelScrolled` → `handle_scroll()`
- Keyboard → Keybinding lookup via `KeyBindings`

### Interaction Modes

- **Pan**: Middle mouse or Shift+Left drag
- **Zoom X**: Ctrl+Left drag horizontal
- **Zoom Y**: Ctrl+Left drag vertical
- **Zoom Rect**: Right drag (box zoom)
- **Pin Toggle**: Left click on/near data point
- **ResetView**: Double-click on plot area
- **Box Select**: Left drag (Select tool)
- **Linked Cursor**: Cross-plot synchronization

## Rendering Pipeline (`widget/frame.rs`)

### Frame Building Steps

1. **Viewport Refresh**: `plot.refresh_viewport()` with padding
2. **Axis Layout**: Update x/y axis label caches
3. **Rect Calculation**: Plot area, axis areas
4. **Transform**: Data ↔ Screen coordinate mapping
5. **Grid**: Major/minor grid lines
6. **Series**: Decimated line/scatter rendering
7. **Regions (NEW)**: Domain-agnostic region rendering
8. **Selection**: Box select, selected regions highlights
9. **Linked Cursor/Brush**: Cross-plot sync visualization
10. **Hover Target**: Pin/point proximity detection

### Region Rendering (`build_regions()`)

```rust
fn build_regions(render, plot, transform, plot_rect, measurer) {
    for region in plot.regions() {
        if !region.visible { continue; }
        
        let screen_bounds = region.screen_bounds(transform)?;
        let clipped = clip_to_plot(screen_bounds, plot_rect);
        
        match region.kind {
            RegionKind::MidiNote { pitch, velocity, ... } => {
                // Rectangle with velocity bar + channel color
            }
            RegionKind::AudioRegion { ... } => {
                // Waveform envelope + region outline
            }
            RegionKind::AutomationPoint { ... } => {
                // Diamond/circle marker
            }
            RegionKind::AutomationCurve { points } => {
                // Interpolated curve segments
            }
            RegionKind::Marker { label, color } => {
                // Triangle marker + text label
            }
            RegionKind::Loop { start, end } => {
                // Brace/bracket with label
            }
            _ => default_rectangle_style()
        }
    }
}
```

## Multi-Plot Linking (`widget/link.rs`)

Synchronizes multiple plots:

- **`PlotLinkGroup`**: Shared state between plots
- **`LinkBinding`**: Per-plot configuration (member_id, options)
- **`PlotLinkOptions`**: link_x, link_y, link_reset, link_cursor, link_brush
- **Published Updates**: Viewport changes, cursor position, brush range
- **Sequence Numbers**: Prevents feedback loops

## Decimation & Performance

- **`DecimationScratch`**: Reusable buffers for LTTB (Largest-Triangle-Three-Buckets)
- **`RenderCacheKey`**: Caches decimated geometry per viewport
- **Progressive rendering**: Only visible data decimated
- **Target point count**: Configurable (default ~2000 points)

## Future Directions & Extensibility

### 1. Plugin Architecture

The action/tool/region systems are designed for extension:

```rust
// Custom action
struct MyCustomAction { ... }
impl PlotAction for MyCustomAction { ... }

// Custom tool
struct MyToolHandler;
impl ToolHandler for MyToolHandler { ... }
registry.register_tool(Tool::Custom("my_tool"), Box::new(MyToolHandler));

// Custom region
RegionKind::Custom("my_type".into(), metadata)
```

### 2. Domain-Specific Applications

| Domain | Regions | Tools | Actions |
|--------|---------|-------|---------|
| DAW Piano Roll | MidiNote, Loop, Marker | Pen, Velocity, Quantize | AddNote, MoveNote, QuantizeSelection |
| Audio Editor | AudioRegion, Fade, Crossfade | Split, Join, Stretch | CutRegion, ApplyFade, TimeStretch |
| Automation | AutomationPoint, AutomationCurve | Draw, Line, Curve | AddPoint, SetCurveType, SmoothCurve |
| Video Editor | ClipRegion, Transition | Razor, Select | CutClip, AddTransition |
| Scientific | Marker, Range, Annotation | Measure, Annotate | AddMarker, FitCurve |

### 3. Advanced Interaction Patterns

- **Gesture Recognition**: Multi-touch, pinch-zoom, rotate
- **Context Menus**: Right-click actions based on hover target
- **Drag-and-Drop**: Between plots, from file browser
- **Scripting**: Lua/JS bindings for batch operations
- **Real-time Collaboration**: CRDT-based multi-user editing

### 4. Rendering Enhancements

- **GPU Acceleration**: wgpu backend for large datasets
- **Level-of-Detail**: Automatic detail reduction at zoom levels
- **Instanced Rendering**: For thousands of markers/regions
- **Async Decimation**: Background thread for non-blocking UI

### 5. Data Integration

- **Streaming Sources**: Async data pipes, WebSocket, OSC, MIDI
- **Format Support**: CSV, Parquet, HDF5, TDMS, WAV, MIDI
- **Time Synchronization**: Multiple time bases, sample-accurate sync
- **Derived Series**: Math operations, FFT, filters, envelopes

## Configuration

### PlotViewConfig (widget/config.rs)

```rust
pub struct PlotViewConfig {
    pub drag_threshold_px: f32,      // 4.0
    pub pin_threshold_px: f32,       // 12.0
    pub unpin_threshold_px: f32,     // 18.0
    pub padding_frac: f64,           // 0.05
    pub min_padding: f64,            // 1e-6
    pub show_legend: bool,           // true
    pub show_hover: bool,            // true
}
```

### SnapConfig (snap.rs)

```rust
pub struct SnapConfig {
    pub enabled: bool,
    pub grid: GridSpec,
    pub threshold_px: f32,
    pub multi_grid: Vec<GridSpec>,
}
```

### KeyBindings (widget/keybindings.rs)

```rust
let mut bindings = KeyBindings::default();
bindings.bind(Key::Z, Modifiers::CTRL, StandardAction::Undo);
bindings.bind_chord(Chord::ctrl_shift(Key::Z), StandardAction::Redo);
```

## Testing & Quality

- **Unit tests**: Geometry, transform, interaction helpers
- **Integration tests**: Plot building, event handling, rendering
- **Property tests**: Decimation accuracy, transform invertibility
- **Visual regression**: Frame comparison for rendering

## Dependencies

- **iced 0.15-dev** (git master): Canvas, widget, core
- **chrono 0.4**: Time formatting (with serde feature)
- **serde 1.0**: Serialization for regions, actions, domain types
- **serde_json 1.0**: Clipboard JSON format

## Version History

- **0.1.0**: Initial port from gpui-liveplot
- **0.2.0**: Action framework, keybindings, undo/redo
- **0.3.0**: Region system, selection, snap/grid
- **0.4.0**: Tool system, MIDI/Audio/Automation domains
- **0.5.0**: Documentation, examples, API stabilization (current)

---

*This summary covers the implementation as of the current development state. For API usage, see `USAGE_GUIDE.md`. For example applications, see `bin/advanced_future_directions.rs`.*