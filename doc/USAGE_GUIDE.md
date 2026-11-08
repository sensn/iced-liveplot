# iced-liveplot Usage Guide

## Table of Contents

1. [Quick Start](#quick-start)
2. [Basic Plot Creation](#basic-plot-creation)
3. [Interactive Features](#interactive-features)
4. [Actions & Undo/Redo](#actions--undoredo)
5. [Keybindings](#keybindings)
6. [Regions](#regions)
7. [Selection](#selection)
8. [Snap & Grid](#snap--grid)
9. [Tools](#tools)
10. [Domain Modules](#domain-modules)
    - [MIDI / Piano Roll](#midi--piano-roll)
    - [Audio / Waveform](#audio--waveform)
    - [Automation](#automation)
11. [Multi-Plot Linking](#multi-plot-linking)
12. [Customization](#customization)
13. [Complete Example](#complete-example)

---

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
iced = { git = "https://github.com/iced-rs/iced", branch = "master", features = ["canvas", "tokio", "advanced"] }
```

Minimal example:

```rust
use iced_liveplot::{Plot, PlotBuilder, Series, widget::PlotWidget};
use iced::{Element, Application, Command, Theme};

fn main() -> iced::Result {
    iced::application("My Plot", App::update, App::view)
        .theme(|_| Theme::Dark)
        .run()
}

struct App {
    plot: Plot,
}

impl Application for App {
    type Message = iced_liveplot::widget::Message;
    type Theme = Theme;
    type Executor = iced::executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Self::Message>) {
        let mut plot = PlotBuilder::new()
            .add_series(Series::line("Signal").with_data(&[1.0, 2.0, 1.5, 3.0, 2.5]))
            .build();
        
        (Self { plot }, Command::none())
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        self.plot.update(message);
        Command::none()
    }

    fn view(&self) -> Element<Self::Message> {
        PlotWidget::new(&self.plot).into()
    }
}
```

---

## Basic Plot Creation

### PlotBuilder

```rust
use iced_liveplot::{PlotBuilder, Series, SeriesKind, LineStyle, MarkerStyle};

let plot = PlotBuilder::new()
    .theme(my_theme)
    .x_axis_config(|axis| axis.label("Time (s)"))
    .y_axis_config(|axis| axis.label("Amplitude"))
    .view(Viewport::new(0.0..10.0, -1.0..1.0))
    .add_series(
        Series::line("Channel 1")
            .with_data(&data_points)           // Indexed: x = 0,1,2...
            .color([1.0, 0.5, 0.0, 1.0])
            .line_width(2.0)
    )
    .add_series(
        Series::scatter("Peaks")
            .with_xy_data(&[(1.0, 0.9), (3.0, 0.95), (5.0, 0.8)])
            .marker(MarkerStyle::Circle(4.0))
    )
    .build();
```

### Series Types

```rust
// Line series (connected points)
Series::line("Name")
    .with_data(&[1.0, 2.0, 3.0])           // Indexed
    .with_xy_data(&[(0.0, 1.0), (1.0, 2.0)]) // Explicit x,y
    .style(LineStyle { width: 2.0, color: ..., dash: None })

// Scatter series (individual points)
Series::scatter("Name")
    .with_data(&[1.0, 2.0])
    .style(MarkerStyle::Circle(4.0))        // Circle, Square, Triangle, Cross, Diamond
    .color([1.0, 0.0, 0.0, 1.0])

// Shared data (multiple plots viewing same stream)
let shared = Series::line("Live").shared();
plot1.add_series(shared.clone());
plot2.add_series(shared.clone());  // Same data, independent views
```

---

## Interactive Features

### Mouse Interactions (Default)

| Action | Mouse | Description |
|--------|-------|-------------|
| Pan | Middle drag / Shift+Left drag | Move viewport |
| Zoom X | Ctrl+Left drag horizontal | Horizontal zoom |
| Zoom Y | Ctrl+Left drag vertical | Vertical zoom |
| Box Zoom | Right drag | Zoom to rectangle |
| Add Pin | Left click near point | Toggle pin at data point |
| Reset View | Double-click | Reset to auto-fit |
| Box Select | Left drag (Select tool) | Select multiple items |

### Pin System

Pins mark specific data points with persistent labels:

```rust
// Programmatic pin management
plot.add_pin(Pin::new(series_id, point_index));
plot.remove_pin(&pin);
plot.clear_pins();

// Pin configuration
config.pin_threshold_px = 12.0;     // Click tolerance for adding
config.unpin_threshold_px = 18.0;   // Click tolerance for removing
```

---

## Actions & Undo/Redo

### Using the Action System

```rust
use iced_liveplot::widget::{ActionDispatcher, actions::{AddPin, SetViewport, BatchAction}};

let mut dispatcher = ActionDispatcher::new()
    .with_max_history(100)
    .with_group_window(Duration::from_millis(500));

// Execute single action
let result = dispatcher.execute(
    AddPin { pin: Pin::new(series_id, 42) },
    &mut plot, &mut state, &config, &mut clipboard
);

// Execute batch (grouped for single undo)
let batch = BatchAction::new("Move Selection")
    .add(MovePin { pin, delta: Point::new(0.5, 0.1) })
    .add(MoveRegion { id, delta: Point::new(0.5, 0.0) });
dispatcher.execute(batch, &mut plot, &mut state, &config, &mut clipboard);

// Undo/Redo
if dispatcher.can_undo() {
    dispatcher.undo(&mut plot, &mut state, &config, &mut clipboard);
}
if dispatcher.can_redo() {
    dispatcher.redo(&mut plot, &mut state, &config, &mut clipboard);
}
```

### Built-in Actions

```rust
// Pin actions
AddPin { pin }
RemovePin { pin }
TogglePin { pin }
MovePin { pin, delta: Point }

// View actions
SetViewport { viewport }
ResetView
ZoomToRect { rect: ScreenRect }

// Selection actions
SelectAll
ClearSelection
ExtendSelection { anchor, target }

// Region actions
AddRegion { region }
RemoveRegion { id }
MoveRegion { id, delta }
ResizeRegion { id, new_bounds }

// Clipboard actions
CopySelection
CutSelection
PasteClipboard { position: Point }

// Batch
BatchAction::new("Description").add(action1).add(action2)
```

### Creating Custom Actions

```rust
use iced_liveplot::widget::actions::{PlotAction, ActionContext, ActionResult, ActionError};

#[derive(Debug)]
struct QuantizeSelection {
    grid_division: u32,
}

impl PlotAction for QuantizeSelection {
    fn name(&self) -> &'static str { "Quantize Selection" }
    
    fn can_undo(&self) -> bool { true }
    
    fn execute(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        // Quantize selected points/regions to grid
        let grid = GridSpec::new(TimeBase::Beats, self.grid_division);
        for (series_id, indices) in &ctx.state.selection.points {
            // ... modify points ...
        }
        ActionResult::Redraw
    }
    
    fn undo(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        // Restore previous positions (store in action or use memento)
        ActionResult::Redraw
    }
}
```

---

## Keybindings

### Default Bindings

| Key | Action |
|-----|--------|
| `Ctrl+Z` / `Cmd+Z` | Undo |
| `Ctrl+Y` / `Cmd+Shift+Z` | Redo |
| `Ctrl+C` | Copy |
| `Ctrl+X` | Cut |
| `Ctrl+V` | Paste |
| `Delete` | Delete selection |
| `Ctrl+A` | Select All |
| `Escape` | Deselect All |
| `+` / `=` | Zoom In |
| `-` | Zoom Out |
| `0` | Zoom Fit |
| `R` | Reset View |
| Arrow Keys | Pan |
| `1`-`9` | Switch Tool (Select, Pen, Eraser, Move, Scale, Stretch, Split, Join, Draw) |
| `G` | Toggle Grid |
| `S` | Toggle Snap |

### Custom Keybindings

```rust
use iced_liveplot::widget::{KeyBindings, StandardAction, Chord};
use iced::keyboard::{Key, Modifiers};

let mut bindings = KeyBindings::default();

// Simple key
bindings.bind(Key::Q, Modifiers::empty(), StandardAction::ToolSelect);

// With modifiers
bindings.bind(Key::Z, Modifiers::CTRL, StandardAction::Undo);
bindings.bind(Key::Y, Modifiers::CTRL | Modifiers::SHIFT, StandardAction::Redo);

// Chord (multi-key)
bindings.bind_chord(Chord::ctrl(Key::G), StandardAction::ToggleGrid);
bindings.bind_chord(Chord::ctrl_shift(Key::S), StandardAction::ToggleSnap);

// Remove binding
bindings.unbind(Key::Delete);

// Check binding
if let Some(action) = bindings.get_action(Key::Z, Modifiers::CTRL) {
    println!("Ctrl+Z -> {:?}", action);
}
```

### Integration with Program

```rust
// In your update() method
fn update(&mut self, message: Message) -> Command<Message> {
    match message {
        Message::KeyPressed(key, modifiers) => {
            if let Some(action) = self.keybindings.get_action(key, modifiers) {
                self.dispatcher.execute_standard(action, &mut self.plot, ...);
            }
        }
        _ => {}
    }
    Command::none()
}
```

---

## Regions

### Creating Regions

```rust
use iced_liveplot::{RegionId, RegionKind, DataRegion, RegionStyle, LabelPosition};

// MIDI Note region
let midi_note = DataRegion::new(
    RegionId::new(),
    RegionKind::MidiNote {
        pitch: 60,           // Middle C
        velocity: 100,       // 0-127
        channel: 1,          // 1-16
        note_name: Some("C4".into()),
    },
    1.0,     // start time (seconds)
    0.5,     // duration
).with_style(RegionStyle {
    fill: Some([0.4, 0.6, 1.0, 0.7].into()),
    stroke: Some([0.2, 0.4, 0.8, 1.0].into()),
    stroke_width: 1.0,
    label_color: Some(Color::WHITE),
    label_position: LabelPosition::Center,
});

// Audio region
let audio_region = DataRegion::new(
    RegionId::new(),
    RegionKind::AudioRegion {
        source: AudioSource::Buffer(audio_buffer),
        gain: 1.0,
        fade_in: 0.01,
        fade_out: 0.05,
        pitch_shift: 0.0,
        time_stretch: 1.0,
    },
    0.0,     // start
    5.0,     // duration
);

// Automation point
let auto_point = DataRegion::new(
    RegionId::new(),
    RegionKind::AutomationPoint {
        parameter_id: "filter_cutoff".into(),
        curve_type: CurveType::Linear,
    },
    2.5,     // time
    0.0,     // duration (point)
).with_style(RegionStyle {
    fill: Some([1.0, 0.6, 0.2, 0.9].into()),
    ..Default::default()
});

// Marker
let marker = DataRegion::new(
    RegionId::new(),
    RegionKind::Marker {
        label: "Chorus".into(),
        color: [1.0, 0.8, 0.0, 1.0].into(),
    },
    30.0,    // time
    0.0,     // duration
);

// Loop region
let loop_region = DataRegion::new(
    RegionId::new(),
    RegionKind::Loop { start: 10.0, end: 20.0 },
    10.0,    // start
    10.0,    // duration
);
```

### Adding Regions to Plot

```rust
// Direct API
plot.add_region(midi_note);
plot.add_region(audio_region);

// Via action (with undo)
dispatcher.execute(
    AddRegion { region: midi_note },
    &mut plot, &mut state, &config, &mut clipboard
);

// Remove
plot.remove_region(&region_id);
dispatcher.execute(RemoveRegion { id: region_id }, ...);
```

### Region Styling

```rust
RegionStyle {
    fill: Option<Color>,           // Interior fill
    stroke: Option<Color>,         // Border color
    stroke_width: f32,             // Border width
    label_color: Option<Color>,    // Label text color
    label_position: LabelPosition, // Center, Top, Bottom, Left, Right, TopLeft, etc.
}
```

---

## Selection

### Multi-Selection

```rust
use iced_liveplot::{Selection, SelectionAnchor, AnchorKind, Pin, RegionId, SeriesId};

// Selection state
let mut selection = Selection::new();

// Select pins
selection.pins.insert(Pin::new(series_id, 10));
selection.pins.insert(Pin::new(series_id, 15));

// Select regions
selection.regions.insert(region_id_1);
selection.regions.insert(region_id_2);

// Select data points
selection.points.entry(series_id).or_default().insert(42);
selection.points.entry(series_id).or_default().insert(43);

// Anchor for shift-extend
selection.anchor = Some(SelectionAnchor {
    kind: AnchorKind::Pin(Pin::new(series_id, 10)),
    screen_pos: ScreenPoint::new(100.0, 200.0),
});

// Check if empty
if selection.is_empty() { ... }

// Clear
selection.clear();
```

### Keyboard Modifiers

| Modifier | Behavior |
|----------|----------|
| None | Replace selection |
| Shift | Extend from anchor |
| Ctrl/Cmd | Toggle item |
| Shift+Ctrl | Extend + toggle |

---

## Snap & Grid

### Configuration

```rust
use iced_liveplot::{SnapConfig, SnapEngine, GridSpec, TimeBase, SnapType};

let snap_config = SnapConfig {
    enabled: true,
    grid: GridSpec {
        time_base: TimeBase::Beats,
        division: 16,        // 16th notes
        swing: 0.0,          // 0.0 = straight, 0.5 = triplet
        offset: 0.0,
    },
    threshold_px: 10.0,      // Snap distance in pixels
    multi_grid: vec![
        GridSpec::new(TimeBase::Beats, 4),   // Quarter notes
        GridSpec::new(TimeBase::Bars, 1),    // Bar lines
    ],
};

let snap_engine = SnapEngine::new(snap_config);
```

### Snapping Points

```rust
// Snap a data point to nearest grid line
let result = snap_engine.snap_point(data_point, &viewport, &transform);

if result.snapped {
    println!("Snapped from {} to {} (grid line {})", 
        data_point.x, result.value.x, result.grid_index);
    
    // result.distance = distance in data units
    // result.snap_type = SnapType::Grid / SnapType::Point / SnapType::Region
}

// Snap to specific grid
let result = snap_engine.snap_to_grid(data_point, &grid_spec, &viewport, &transform);

// Snap to nearest region boundary
let result = snap_engine.snap_to_nearest(data_point, &regions, &viewport, &transform);
```

### Musical Time

```rust
// Convert time to musical position
let beats = snap_engine.time_to_beats(time_seconds, tempo_bpm);
let (bar, beat, tick) = snap_engine.beats_to_musical(beats, ticks_per_beat);

// Format for display
let label = snap_engine.format_musical(bar, beat, ticks_per_beat);  // "3.2.480"
```

---

## Tools

### Built-in Tools

```rust
use iced_liveplot::widget::{Tool, ToolRegistry, ToolState};

let mut tool_state = ToolState::new(Tool::Select);
let mut registry = ToolRegistry::default();

// Switch tool
tool_state.set_tool(Tool::Pen);

// Get current tool cursor
let cursor = tool_state.current_tool().cursor();  // iced::mouse::Interaction

// Tool descriptions for UI
for tool in Tool::all() {
    println!("{}: {}", tool.name(), tool.description());
}
```

### Custom Tool Handler

```rust
use iced_liveplot::widget::tools::{ToolHandler, Tool, ToolContext, ToolResult};

struct MyCustomTool;

impl ToolHandler for MyCustomTool {
    fn on_mouse_down(&mut self, ctx: &mut ToolContext, pos: ScreenPoint, button: MouseButton) -> ToolResult {
        // Handle press
        ToolResult::Continue
    }
    
    fn on_mouse_move(&mut self, ctx: &mut ToolContext, pos: ScreenPoint) -> ToolResult {
        // Handle drag
        ToolResult::Continue
    }
    
    fn on_mouse_up(&mut self, ctx: &mut ToolContext, pos: ScreenPoint, button: MouseButton) -> ToolResult {
        // Handle release - create action
        let action = MyCustomAction { ... };
        ToolResult::Action(Box::new(action))
    }
    
    fn cursor(&self) -> iced::mouse::Interaction {
        iced::mouse::Interaction::Crosshair
    }
}

// Register
registry.register_tool(Tool::Custom("my_tool".into()), Box::new(MyCustomTool));
```

---

## Domain Modules

### MIDI / Piano Roll

```rust
use iced_liveplot::midi::{MidiNote, MidiSequence, MidiTrack, PianoRollConfig, PianoRollRenderer};

// Create notes
let note = MidiNote::new(60, 100, 1.0, 0.5)  // pitch, velocity, start, duration
    .with_channel(1);

// Create sequence
let mut sequence = MidiSequence::new();
sequence.add_note(note);
sequence.add_note(MidiNote::new(64, 90, 1.5, 0.5));
sequence.quantize(16);  // Snap to 16th notes

// Create track
let track = MidiTrack::new("Lead Synth")
    .with_sequence(sequence)
    .with_color([0.4, 0.6, 1.0, 1.0].into())
    .with_channel(1);

// Piano roll configuration
let config = PianoRollConfig {
    show_note_names: true,
    show_velocity: true,
    note_height: 12.0,
    octave_range: 0..10,
    drum_map: None,
};

// Render
let renderer = PianoRollRenderer::new(config);
renderer.render(&track, &transform, plot_rect, &mut render_list);
```

### Audio / Waveform

```rust
use iced_liveplot::audio::{AudioRegion, AudioClip, WaveformRenderer, AudioSource};

// Create audio region from buffer
let source = AudioSource::Buffer(Arc::new(AudioBuffer::new(samples, sample_rate)));
let region = AudioRegion::new(source, 0.0, duration)
    .with_gain(0.8)
    .with_fades(0.01, 0.05)
    .with_pitch_shift(2.0)      // Semitones
    .with_time_stretch(1.2);    // 1.0 = normal

// Create clip (timeline reference)
let clip = AudioClip::new(region.id, 5.0);  // Start at 5s

// Waveform rendering with decimation
let renderer = WaveformRenderer::new()
    .with_color([0.6, 0.8, 0.4, 1.0].into())
    .with_max_points(2000);

renderer.render(&region, &transform, plot_rect, &mut render_list);
```

### Automation

```rust
use iced_liveplot::automation::{AutomationLane, AutomationPoint, CurveType, AutomationRenderer};

// Create automation lane
let mut lane = AutomationLane::new("filter_cutoff")
    .with_range(0.0..1.0)
    .with_color([1.0, 0.6, 0.2, 1.0].into())
    .with_style(AutomationStyle {
        line_width: 2.0,
        point_radius: 4.0,
        show_points: true,
    });

// Add points with curves
lane.add_point(AutomationPoint::new(0.0, 0.5, CurveType::Linear));
lane.add_point(AutomationPoint::new(1.0, 0.8, CurveType::Exponential));
lane.add_point(AutomationPoint::new(2.0, 0.3, CurveType::Sine));
lane.add_point(AutomationPoint::new(3.0, 0.9, CurveType::Bezier { 
    cp1: Point::new(2.5, 0.5), 
    cp2: Point::new(2.8, 0.7) 
}));

// Evaluate at time
let value = lane.value_at(1.5);  // Interpolated value

// Render
let renderer = AutomationRenderer::new();
renderer.render(&lane, &transform, plot_rect, &mut render_list);
```

---

## Multi-Plot Linking

```rust
use iced_liveplot::widget::link::{PlotLinkGroup, LinkBinding, PlotLinkOptions};

let link_group = PlotLinkGroup::new();

// Plot 1
let link1 = link_group.bind(PlotLinkOptions {
    link_x: true,
    link_y: false,
    link_reset: true,
    link_cursor: true,
    link_brush: true,
});
program1.set_link(link1);

// Plot 2
let link2 = link_group.bind(PlotLinkOptions {
    link_x: true,
    link_y: true,
    link_reset: false,
    link_cursor: true,
    link_brush: false,
});
program2.set_link(link2);

// Now: panning plot1 horizontally pans plot2
//      vertical zoom is independent
//      reset view on plot1 resets plot2
//      hover cursor syncs across both
```

---

## Customization

### Themes

```rust
use iced_liveplot::style::Theme;

let dark_theme = Theme::dark();
let light_theme = Theme::light();

// Custom theme
let custom = Theme {
    background: [0.1, 0.1, 0.12, 1.0].into(),
    axis: [0.7, 0.7, 0.75, 1.0].into(),
    grid_major: [0.3, 0.3, 0.35, 1.0].into(),
    grid_minor: [0.2, 0.2, 0.25, 1.0].into(),
    hover_bg: [0.15, 0.15, 0.18, 0.95].into(),
    hover_text: [0.95, 0.95, 0.95, 1.0].into(),
    selection_fill: [0.3, 0.5, 0.9, 0.3].into(),
    selection_border: [0.3, 0.5, 0.9, 1.0].into(),
    series_colors: vec![
        [0.9, 0.4, 0.3, 1.0].into(),
        [0.3, 0.8, 0.4, 1.0].into(),
        [0.4, 0.6, 1.0, 1.0].into(),
        [1.0, 0.8, 0.2, 1.0].into(),
    ],
    ..Default::default()
};

plot.set_theme(custom);
```

### PlotViewConfig

```rust
use iced_liveplot::widget::config::PlotViewConfig;

let config = PlotViewConfig {
    drag_threshold_px: 4.0,
    pin_threshold_px: 12.0,
    unpin_threshold_px: 18.0,
    padding_frac: 0.05,
    min_padding: 1e-6,
    show_legend: true,
    show_hover: true,
};

program.set_config(config);
```

---

## Complete Example

See `bin/advanced_future_directions.rs` for a comprehensive example demonstrating:

- Multiple linked plots (waveform, piano roll, automation)
- Full action system with undo/redo
- Custom keybindings
- Region editing (MIDI notes, audio clips, automation)
- Tool switching (Select, Pen, Eraser, Move, Draw)
- Snap to musical grid
- Multi-selection with keyboard modifiers
- Real-time cursor synchronization

```bash
cargo run --bin advanced_future_directions
```

### Key Files

| File | Purpose |
|------|---------|
| `bin/basic.rs` | Minimal plot with series |
| `bin/advanced.rs` | Interactive plot with pins, linking |
| `bin/advanced_future_directions.rs` | Full feature showcase |

---

## Migration from gpui-liveplot

| gpui-liveplot | iced-liveplot |
|---------------|---------------|
| `gpui::View` | `iced::widget::canvas::Program` |
| `on_mouse_down` | `handle_mouse_down` in events.rs |
| `Plot::pin_mut()` | `plot.pins_mut()` |
| `Viewport` | `view::Viewport` (same) |
| `Transform` | `transform::Transform` (same) |
| `Decimation` | `datasource::DecimationScratch` |

---

## Performance Tips

1. **Use shared series** for multiple views of same data
2. **Enable decimation** (default) for large datasets (>10k points)
3. **Limit render cache** with `RenderCacheKey` for dynamic data
4. **Batch actions** for multi-object edits
5. **Use `Region::visible`** to hide off-screen regions
6. **Profile with `cargo build --release`** and `perf`/`Instruments`

---

## Troubleshooting

### Plot not updating
- Ensure `Program::update()` calls `plot.update(message)`
- Call `program.request_redraw()` after state changes

### Pins not appearing
- Check `pin_threshold_px` in config
- Verify series has data at that index
- Ensure `plot.pins()` is not empty

### Double-click not working
- Uses manual detection in `events.rs` (300ms / 6px)
- Not using `mouse_area` wrapper
- Check `LastClick` tracking in state

### Linking not syncing
- Verify same `PlotLinkGroup` instance
- Check `PlotLinkOptions` match desired sync axes
- Sequence numbers prevent feedback loops

---

## API Reference

### Core Types

- `Plot` - Main plot model
- `PlotBuilder` - Fluent plot construction
- `Series` - Data series (line/scatter)
- `SeriesId` - Stable series identifier
- `Pin` - (series_id, point_index) pair
- `Viewport` - Visible data range
- `Transform` - Data ↔ Screen mapping

### Widget Types

- `Program` - Canvas program implementation
- `PlotWidget` - Iced element wrapper
- `Message` - Widget messages (Tick, Redraw, ToggleSeries, ResetView)
- `PlotUiState` - Interaction state (internal)

### Action Types

- `ActionDispatcher` - Execute/undo/redo
- `History` - High-level history manager
- `Clipboard` - Copy/paste buffer
- `PlotAction` - Action trait
- `StandardAction` - Predefined actions

### Interaction Types

- `KeyBindings` - Keyboard shortcuts
- `KeyBinding` - Single binding
- `Chord` - Key + modifiers
- `Selection` - Multi-selection state
- `Selectable` - Trait for selectable items

### Region Types

- `RegionId` - Unique region identifier
- `RegionKind` - Region type enum
- `DataRegion` - Region with metadata
- `RegionStyle` - Visual styling
- `RegionRegistry` - Type registration

### Snap Types

- `SnapEngine` - Snapping logic
- `SnapConfig` - Configuration
- `GridSpec` - Grid specification
- `TimeBase` - Time units
- `SnapResult` - Snap outcome

### Tool Types

- `Tool` - Tool enum
- `ToolRegistry` - Tool management
- `ToolState` - Current tool state
- `ToolHandler` - Custom behavior trait
- `ToolContext` - Handler context

---

## License

MIT OR Apache-2.0