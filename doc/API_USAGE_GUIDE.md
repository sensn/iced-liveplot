# API Usage Guide: iced-liveplot

## Table of Contents
1. [Quick Start](#quick-start)
2. [Creating a Plot](#creating-a-plot)
3. [Series & Data](#series--data)
4. [Axes Configuration](#axes-configuration)
5. [Interactions](#interactions)
6. [Multi-Plot Linking](#multi-plot-linking)
7. [Theming](#theming)
8. [Advanced Patterns](#advanced-patterns)

---

## Quick Start

```rust
use iced_liveplot::{Plot, Series, widget::plot};

fn main() -> iced::Result {
    let mut plot = Plot::builder()
        .x_axis(|a| a.title("Time (s)"))
        .y_axis(|a| a.title("Amplitude"))
        .series(&Series::line("Signal"))
        .build();

    // Add some data
    plot.series_mut()[0].extend_y(0..100).unwrap();

    iced::application("Live Plot", update, view)
        .run()
}

#[derive(Debug, Clone)]
enum Message {
    Plot(iced_liveplot::widget::Message),
}

fn update(message: Message) {
    match message {
        Message::Plot(msg) => { /* handled by widget */ }
    }
}

fn view() -> impl iced::Widget<Message> {
    plot(&plot).map(Message::Plot)
}
```

---

## Creating a Plot

### `Plot::builder()` - Fluent Construction

```rust
let plot = Plot::builder()
    .id("main-plot")                    // Optional: for debugging/linking
    .theme(Theme::Dark)                 // Light/Dark/Custom
    .x_axis(|axis| axis                 // X Axis Configuration
        .title("Time")
        .units("seconds")
        .grid(true)
        .minor_grid(true)
        .formatter(|v| format!("{:.2}", v))
    )
    .y_axis(|axis| axis                 // Y Axis Configuration
        .title("Voltage")
        .units("V")
        .show_zero_line(true)
    )
    .view(View::new(-10.0, 10.0))       // Initial data view
    .series(&series1)
    .series(&series2)
    .build();
```

### `Plot` Methods

| Method | Description |
|--------|-------------|
| `theme(&self)` | Get current theme |
| `set_theme(&mut self, Theme)` | Change theme at runtime |
| `x_axis(&self)` / `y_axis(&self)` | Access axis config (immutable) |
| `view(&self)` | Current data view |
| `viewport(&self)` | Current render viewport (includes padding) |
| `series(&self)` | Immutable series slice |
| `series_mut(&mut self)` | Mutable series slice |
| `add_series(&mut self, &Series)` | Add series to plot |
| `pins(&self)` | Current pinned points |
| `pins_mut(&mut self)` | Mutable pins |
| `data_bounds(&self)` | Union of all series bounds |
| `set_manual_view(&mut self, Viewport)` | Override auto-viewport |
| `reset_view(&mut self)` | Clear manual view, auto-fit |
| `refresh_viewport(&mut self, frac, min)` | Recalculate auto-viewport |

---

## Series & Data

### Creating Series

```rust
use iced_liveplot::{Series, SeriesKind, render::{LineStyle, MarkerStyle}};

// Line series (indexed X: 0, 1, 2...)
let mut line = Series::line("Temperature");

// Scatter series (indexed X)
let mut scatter = Series::scatter("Samples");

// Explicit X values
let explicit = Series::from_iter_points(
    "Explicit",
    [(0.0, 1.0), (1.5, 2.0), (3.0, 1.5)].map(Point::new),
    SeriesKind::Line(LineStyle::default())
);

// From function
let fn_series = Series::from_explicit_callback(
    "Sine",
    |x| x.sin(),
    Range::new(0.0, 10.0),
    1000,
    SeriesKind::Line(LineStyle::default())
);

// Shared data (same underlying buffer, different SeriesId)
let shared = line.share();
shared.with_kind(SeriesKind::Scatter(MarkerStyle::default()));
```

### Appending Data (Streaming)

```rust
// Indexed series (implicit X)
line.push_y(23.5).unwrap();
line.extend_y([24.0, 23.8, 24.1]).unwrap();

// Explicit series (explicit X)
explicit.push_point(Point::new(10.5, 100.0)).unwrap();
explicit.extend_points([
    Point::new(11.0, 101.0),
    Point::new(11.5, 99.5),
]).unwrap();

// Check bounds
if let Some(bounds) = line.bounds() {
    println!("X: {:?}, Y: {:?}", bounds.x, bounds.y);
}

// Generation for cache invalidation
let gen = line.generation();
```

### Series Methods Reference

| Method | Return | Description |
|--------|--------|-------------|
| `id()` | `SeriesId` | Unique identifier |
| `name()` | `&str` | Display name |
| `kind()` | `&SeriesKind` | Line/Scatter + style |
| `with_kind(SeriesKind)` | `Self` | Builder: change style |
| `share()` | `Self` | New handle, same data |
| `with_store(R)` | `R` | Read access to store |
| `push_y(f64)` | `Result<usize, AppendError>` | Append indexed |
| `extend_y(I)` | `Result<usize, AppendError>` | Batch indexed |
| `push_point(Point)` | `Result<usize, AppendError>` | Append explicit |
| `extend_points(I)` | `Result<usize, AppendError>` | Batch explicit |
| `bounds()` | `Option<Viewport>` | Data bounds |
| `generation()` | `u64` | Monotonic counter |
| `is_visible()` | `bool` | Visibility flag |
| `set_visible(bool)` | `()` | Toggle visibility |

### `AppendError` Handling

```rust
match series.extend_y(values) {
    Ok(count) => println!("Appended {count} points"),
    Err(AppendError::WrongMode) => eprintln!("Series expects explicit X"),
    Err(AppendError::NonMonotonicX) => eprintln!("X not monotonic (still appended)"),
}
```

---

## Data Source (`datasource/mod.rs`)

### `AppendOnlyData` - Low-Level Storage

```rust
use iced_liveplot::datasource::{AppendOnlyData, XMode, AppendError, Point, Viewport, Range};

// Create
let mut data = AppendOnlyData::indexed();      // Implicit X
let mut data = AppendOnlyData::explicit();     // Explicit X

// Build from iterators
let data = AppendOnlyData::from_iter_y([1.0, 2.0, 3.0]);
let data = AppendOnlyData::from_iter_points([
    Point::new(0.0, 1.0),
    Point::new(1.0, 2.0),
]);

// From callback
let data = AppendOnlyData::from_explicit_callback(
    |x| x.sin(),
    Range::new(0.0, 10.0),
    1000
);

// Append
data.extend_y([4.0, 5.0]).unwrap();       // Indexed
data.extend_points([Point::new(2.0, 3.0)]).unwrap(); // Explicit

// Query
let points = data.points();                // &[Point]
let point = data.point(5);                 // Option<Point>
let len = data.len();
let bounds = data.bounds();                // Option<Viewport>
let range = data.range_by_x(Range::new(1.0, 5.0)); // Index range
let nearest = data.nearest_index_by_x(2.5); // Hit test

// Properties
data.x_mode();           // XMode::Index | Explicit
data.is_monotonic();     // Binary search available?
```

### `SeriesStore` - Store with Decimation

```rust
use iced_liveplot::datasource::{SeriesStore, DecimationScratch};

let mut store = SeriesStore::indexed();  // Default base_chunk = 64
let mut store = SeriesStore::with_base_chunk(data, 128);

// Same append methods as AppendOnlyData
store.extend_y(values).unwrap();

// Decimation for rendering
let mut scratch = DecimationScratch::new();
let target_chunk = 64; // pixels per output point
store.decimate(target_chunk, &mut scratch); // Fills scratch buffer

// Access decimated output
let decimated = scratch.output(); // &[Point]
```

---

## Axes Configuration

### `AxisConfig` - Builder Pattern

```rust
use iced_liveplot::{AxisConfig, AxisFormatter, TickConfig, Range};

let axis = AxisConfig::builder()
    .title("Frequency")
    .units("Hz")
    .formatter(AxisFormatter::new(|v| format!("{:.1}k", v / 1000.0)))
    .tick_config(TickConfig::Linear { 
        target_tick_count: 8,
        min_tick_spacing: 50.0,
    })
    .grid(true)
    .minor_grid(false)
    .show_zero_line(true)
    .show_border(true)
    .label_size(12.0)
    .build();

// Or functional style
let axis = AxisConfig::builder()
    .title("Time")
    .formatter(|v| format!("{:.2}s", v))
    .build();
```

### `TickConfig` - Tick Generation

```rust
use iced_liveplot::TickConfig;

// Linear (default)
TickConfig::Linear {
    target_tick_count: 10,
    min_tick_spacing: 40.0, // pixels
}

// Logarithmic
TickConfig::Logarithmic {
    base: 10.0,
}

// Time-based
TickConfig::Time {
    timezone: chrono_tz::UTC, // or Local
    format: "%H:%M:%S".to_string(),
}
```

### `AxisFormatter` - Custom Formatting

```rust
use iced_liveplot::AxisFormatter;

// Simple closure
AxisFormatter::new(|value| format!("{:.2}", value))

// With units
AxisFormatter::new(|v| format!("{:.1} V", v))

// Scientific
AxisFormatter::scientific(3) // 3 decimal places

// Engineering (SI prefixes)
AxisFormatter::engineering()

// Time
AxisFormatter::time("%H:%M:%S%.3f")
```

---

## Interactions

### Built-in Interactions

| Action | Mouse | Description |
|--------|-------|-------------|
| Pan | Left Drag | Drag plot area |
| Box Zoom | Right Drag | Select zoom rectangle |
| Zoom Wheel | Scroll | Zoom at cursor |
| H-Scroll Zoom | Shift+Scroll | Horizontal zoom |
| Pin/Marker | Left Click | Toggle pin on nearest point |
| Reset View | Double Click | Reset to auto-fit |
| Hover | Move | Tooltip on nearest point |
| Linked Cursor | — | Cross-plot vertical line |
| Linked Brush | — | Cross-plot X range selection |

### `PlotViewConfig` - Interaction Tuning

```rust
use iced_liveplot::PlotViewConfig;

let config = PlotViewConfig {
    pin_threshold_px: 10.0,      // Click distance to pin
    unpin_threshold_px: 15.0,    // Click distance to unpin
    hover_threshold_px: 12.0,    // Hover distance
    drag_threshold_px: 3.0,      // Min drag to start pan
    scroll_zoom_factor: 1.2,     // Wheel zoom speed
    ..Default::default()
};

let plot = Plot::builder()
    .config(config)
    .build();
```

### `Message` Enum - Widget Events

```rust
use iced_liveplot::widget::Message;

match message {
    Message::ResetView => { /* viewport reset */ }
    Message::ViewChanged(Viewport) => { /* manual view set */ }
    Message::CursorX(f64) => { /* linked cursor position */ }
    Message::BrushX(Range) => { /* linked brush range */ }
    Message::PinAdded(Pin) => { /* pin created */ }
    Message::PinRemoved(Pin) => { /* pin removed */ }
    Message::Hovered(Option<HoverTarget>) => { /* hover changed */ }
}
```

### Custom Interaction Handling

```rust
impl Program for MyApp {
    fn with_plot_mut<R>(&self, f: impl FnOnce(&mut Plot) -> R) -> R {
        f(&mut self.plot)
    }
    
    // Handle custom messages from widget
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::PinAdded(pin) => {
                println!("User pinned: {:?}", pin);
                // Save to project, update UI, etc.
            }
            Message::ViewChanged(vp) => {
                self.save_viewport(vp);
            }
            _ => {}
        }
        Task::none()
    }
}
```

---

## Multi-Plot Linking

### `PlotLinkGroup` - Synchronization Group

```rust
use iced_liveplot::{PlotLinkGroup, PlotLinkOptions, PlotLinkKind};

let link_group = PlotLinkGroup::new();

// Option 1: Link everything
let options = PlotLinkOptions::all();

// Option 2: Selective linking
let options = PlotLinkOptions {
    view: PlotLinkKind::Linked,      // Sync pan/zoom
    cursor_x: PlotLinkKind::Linked,  // Sync vertical cursor
    brush_x: PlotLinkKind::Independent, // Don't sync brush
};

// Apply to plots
let plot1 = Plot::builder()
    .with_link_group(link_group.clone(), options.clone())
    .build();

let plot2 = Plot::builder()
    .with_link_group(link_group.clone(), options)
    .build();
```

### Link Behaviors

| Option | `Linked` | `Independent` | `SourceOnly` | `TargetOnly` |
|--------|----------|---------------|--------------|--------------|
| `view` | Bidirectional sync | No sync | Broadcast only | Receive only |
| `cursor_x` | Shared vertical line | Per-plot | Broadcast cursor | Show received |
| `brush_x` | Shared X selection | Per-plot | Broadcast brush | Show received |

---

## Theming

### `Theme` - Built-in Themes

```rust
use iced_liveplot::Theme;

// Presets
Theme::light()
Theme::dark()

// Custom
let custom = Theme {
    background: Color::from_rgb(0.05, 0.05, 0.06),
    plot_background: Color::from_rgb(0.08, 0.08, 0.1),
    grid_color: Color::from_rgba(0.3, 0.3, 0.35, 0.5),
    minor_grid_color: Color::from_rgba(0.2, 0.2, 0.25, 0.3),
    axis_color: Color::WHITE,
    axis_text_color: Color::from_rgb(0.8, 0.8, 0.85),
    series_colors: vec![
        Color::from_rgb(0.2, 0.7, 1.0),   // Blue
        Color::from_rgb(1.0, 0.5, 0.2),   // Orange
        Color::from_rgb(0.3, 0.9, 0.4),   // Green
        Color::from_rgb(1.0, 0.3, 0.6),   // Pink
        Color::from_rgb(0.8, 0.4, 1.0),   // Purple
    ],
    pin_color: Color::from_rgb(1.0, 0.8, 0.0),
    pin_text_color: Color::WHITE,
    selection_color: Color::from_rgba(0.2, 0.7, 1.0, 0.2),
    hover_color: Color::from_rgb(1.0, 0.9, 0.2),
    linked_cursor_color: Color::from_rgba(1.0, 0.5, 0.2, 0.8),
    linked_brush_color: Color::from_rgba(0.2, 0.7, 1.0, 0.15),
    legend_background: Color::from_rgba(0.1, 0.1, 0.12, 0.9),
    legend_text_color: Color::WHITE,
    ..Default::default()
};
```

### Runtime Theme Switching

```rust
plot.set_theme(if is_dark { Theme::dark() } else { Theme::light() });
program.request_redraw();
```

---

## Advanced Patterns

### 1. High-Frequency Streaming

```rust
use std::sync::Arc;
use iced_liveplot::{Series, Plot};

struct AudioPlot {
    plot: Plot,
    series: Series,
    buffer: Vec<f64>,
}

// Data thread
fn data_thread(series: Series, rx: crossbeam::Receiver<f64>) {
    for sample in rx {
        let _ = series.push_y(sample);
    }
}

// UI thread - batch updates
fn update_plot(app: &mut AudioPlot) {
    // Drain buffer and batch append
    if !app.buffer.is_empty() {
        let _ = app.series.extend_y(app.buffer.drain(..));
    }
}
```

### 2. Shared Data Across Plots

```rust
// Master series (e.g., raw audio)
let mut raw = Series::line("Raw Audio");

// Linked plots with different views
let mut overview = raw.share();
overview.with_kind(SeriesKind::Line(LineStyle { 
    width: 0.5, 
    color: Color::from_rgba(0.5, 0.5, 0.5, 0.5) 
}));

let mut detail = raw.share();
detail.with_kind(SeriesKind::Line(LineStyle { width: 1.5 }));

// Different link options per plot
let overview_options = PlotLinkOptions { 
    view: PlotLinkKind::TargetOnly,  // Follow main plot
    cursor_x: PlotLinkKind::Linked,
    ..
};
```

### 3. Custom Pin Actions

```rust
impl Program for MyApp {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::PinAdded(pin) => {
                // Pin = (SeriesId, index, data_coords, screen_coords)
                let (series_id, index) = (pin.series, pin.index);
                
                // Get data value
                self.plot.series().iter()
                    .find(|s| s.id() == series_id)
                    .and_then(|s| s.with_store(|store| store.point(index)))
                    .map(|pt| {
                        println!("Pinned: series={}, index={}, value={}", 
                            series_id.0, index, pt.y);
                        // Open detail view, export, annotate, etc.
                    });
            }
            _ => {}
        }
        Task::none()
    }
}
```

### 4. Decimation Control

```rust
// For very large datasets, control decimation level
use iced_liveplot::datasource::DecimationScratch;

fn render_with_decimation(plot: &Plot, scratch: &mut DecimationScratch) {
    for series in plot.series() {
        series.with_store(|store| {
            // Target ~1 point per 2 pixels
            let target_chunk = (plot_rect.width() / 2.0).max(1.0) as usize;
            store.decimate(target_chunk, scratch);
            // scratch.output() now has decimated points
        });
    }
}
```

---

## Complete Example: Real-Time Oscilloscope

```rust
use iced_liveplot::{
    Plot, Series, AxisConfig, Theme, Viewport, Range,
    widget::{plot, Message as PlotMessage},
    datasource::AppendOnlyData,
    geom::Point,
};

struct Oscilloscope {
    plot: Plot,
    channel: Series,
    sample_rate: f64,
    buffer: Vec<f64>,
}

impl Oscilloscope {
    fn new() -> Self {
        let channel = Series::line("CH1");
        
        let plot = Plot::builder()
            .theme(Theme::dark())
            .x_axis(|a| a.title("Time").units("s").grid(true))
            .y_axis(|a| a.title("Voltage").units("V").show_zero_line(true))
            .view(View::new(0.0, 1.0)) // 1 second window
            .series(&channel)
            .build();
        
        Self {
            plot,
            channel,
            sample_rate: 48000.0,
            buffer: Vec::new(),
        }
    }
    
    fn push_samples(&mut self, samples: &[f64]) {
        // Convert to time-domain points
        let points: Vec<Point> = samples.iter().enumerate()
            .map(|(i, &y)| Point::new(i as f64 / self.sample_rate, y))
            .collect();
        
        self.channel.extend_points(points).unwrap();
        
        // Keep last 5 seconds
        let max_points = (5.0 * self.sample_rate) as usize;
        if self.channel.generation() > max_points as u64 {
            // TODO: Implement rolling window (future feature)
        }
    }
    
    fn view(&self) -> impl iced::Widget<Message> {
        plot(&self.plot).map(Message::Plot)
    }
}

#[derive(Debug, Clone)]
enum Message {
    Plot(PlotMessage),
    AudioData(Vec<f64>),
}

fn update(app: &mut Oscilloscope, message: Message) {
    match message {
        Message::Plot(_) => {} // Handled by widget
        Message::AudioData(samples) => app.push_samples(&samples),
    }
}
```

---

## Error Handling Summary

| Error Type | Source | Recovery |
|------------|--------|----------|
| `AppendError::WrongMode` | Series data mode mismatch | Use correct `push_y` vs `push_point` |
| `AppendError::NonMonotonicX` | Explicit X not sorted | Data still appended; binary search disabled |
| `Pin` not found | `toggle_pin` on empty vec | Returns `false` (was removed) |
| `Viewport` invalid | Manual view with min>max | `reset_view()` or validate before set |

---

## Migration Guide

### From 0.13 to 0.14+

- `Plot::new()` → `Plot::builder().build()`
- `Series::new()` → `Series::line()` / `Series::scatter()`
- Direct `plot.series.push()` → `series.push_y()` / `series.push_point()`
- Axis config: `AxisConfig { .. }` → `AxisConfig::builder().build()`
- Theme: struct literal → `Theme::light()` / `Theme::dark()` / custom struct

---

## Performance Tips

1. **Batch appends**: Use `extend_y()` / `extend_points()` instead of loops
2. **Reuse Series**: `share()` for multi-plot, don't `clone()` for streaming
3. **Limit pins**: Each pin adds render overhead
4. **Decimation**: Automatic, but base_chunk affects memory (64 default)
5. **Theme**: Dark theme slightly faster (less alpha blending)
6. **Link groups**: Minimal overhead, use `Independent` when not needed