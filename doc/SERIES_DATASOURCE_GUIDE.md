# How-To Guide: Series & DataSource

## Overview

This guide covers the two foundational data abstractions in `iced-liveplot`:
- **Series** (`src/series.rs`): High-level, thread-safe data series for plotting
- **DataSource** (`src/datasource/mod.rs`): Low-level append-only storage with decimation

---

## Part 1: Working with Series

### Series Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        Series                               │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐  │
│  │   SeriesId   │  │    Name      │  │   SeriesKind     │  │
│  │  (unique)    │  │  "Sensor 1"  │  │ Line / Scatter   │  │
│  └──────────────┘  └──────────────┘  └──────────────────┘  │
│                          │                                   │
│                          ▼                                   │
│  ┌────────────────────────────────────────────────────────┐  │
│  │              Arc<RwLock<SeriesStore>>                  │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌───────────────┐  │  │
│  │  │ AppendOnly  │  │  Summary    │  │  Generation   │  │  │
│  │  │   Data      │  │  (Decimate) │  │   Counter     │  │  │
│  │  └─────────────┘  └─────────────┘  └───────────────┘  │  │
│  └────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### 1.1 Creating Series

#### Line Series (most common)
```rust
use iced_liveplot::{Series, SeriesKind, render::LineStyle};

// Default line style
let mut temp = Series::line("Temperature");

// Custom line style
let mut voltage = Series::line("Voltage").with_kind(SeriesKind::Line(LineStyle {
    color: Color::from_rgb(1.0, 0.3, 0.3),
    width: 2.0,
    dash: None, // or Some(&[5.0, 3.0])
}));
```

#### Scatter Series
```rust
use iced_liveplot::render::MarkerStyle;

let mut samples = Series::scatter("Measurements");

// Custom markers
let mut events = Series::scatter("Events").with_kind(SeriesKind::Scatter(MarkerStyle {
    color: Color::from_rgb(1.0, 1.0, 0.0),
    radius: 4.0,
    shape: MarkerShape::Circle, // Circle, Square, Triangle, Cross, Diamond
}));
```

#### From Existing Data
```rust
use iced_liveplot::{AppendOnlyData, Point, Viewport, Range};

// Build data separately
let data = AppendOnlyData::from_iter_points([
    Point::new(0.0, 1.0),
    Point::new(1.0, 3.0),
    Point::new(2.0, 2.0),
]);

let series = Series::with_data("Custom", data, SeriesKind::Line(LineStyle::default()));
```

#### From Iterators (Convenience)
```rust
// Indexed Y values (X = 0, 1, 2...)
let indexed = Series::from_iter_y("Sensor", [20.0, 21.0, 20.5, 22.0], 
    SeriesKind::Line(LineStyle::default()));

// Explicit points
let explicit = Series::from_iter_points("Trajectory", 
    [(0.0, 0.0), (1.0, 2.0), (2.0, 1.0), (3.0, 3.0)].map(Point::new),
    SeriesKind::Scatter(MarkerStyle::default())
);

// Mathematical function
let sine = Series::from_explicit_callback(
    "Sine Wave",
    |x| x.sin(),
    Range::new(0.0, 10.0),
    1000, // sample count
    SeriesKind::Line(LineStyle::default())
);
```

### 1.2 Appending Data (Streaming)

#### Indexed Series (Implicit X)
```rust
let mut series = Series::line("Stream");

// Single point
series.push_y(42.0).unwrap();

// Batch (preferred for performance)
series.extend_y([1.0, 2.0, 3.0, 4.0, 5.0]).unwrap();

// From iterator
series.extend_y((0..1000).map(|i| (i as f64).sin())).unwrap();
```

#### Explicit Series (Explicit X)
```rust
let mut series = Series::scatter("Events");

// Single point
series.push_point(Point::new(10.5, 100.0)).unwrap();

// Batch (must be monotonic X for optimal performance)
series.extend_points([
    Point::new(11.0, 101.0),
    Point::new(11.5, 99.5),
    Point::new(12.0, 102.0),
]).unwrap();

// Non-monotonic X still works but disables binary search
let result = series.extend_points([
    Point::new(10.0, 98.0), // Goes backward!
    Point::new(13.0, 103.0),
]);
// result = Err(AppendError::NonMonotonicX) but points ARE appended
```

#### Error Handling
```rust
use iced_liveplot::datasource::AppendError;

match series.extend_y(values) {
    Ok(count) => println!("Appended {count} points"),
    Err(AppendError::WrongMode) => {
        eprintln!("Series is explicit mode, use push_point/extend_points");
    }
    Err(AppendError::NonMonotonicX) => {
        eprintln!("X values not monotonic - range queries will be slower");
        // Data was still appended!
    }
}
```

### 1.3 Sharing Data Between Series (Critical for Performance)

```rust
// Create master series
let mut master = Series::line("Raw Data");

// Create shared views with DIFFERENT SeriesId but SAME data
let overview = master.share();      // New SeriesId, same Arc<RwLock>
let detail = master.share();        // Another view

// Different styling per view
overview.with_kind(SeriesKind::Line(LineStyle {
    width: 0.5,
    color: Color::from_rgba(0.5, 0.5, 0.5, 0.5),
}));

detail.with_kind(SeriesKind::Line(LineStyle {
    width: 2.0,
    color: Color::from_rgb(1.0, 1.0, 1.0),
}));

// Append to ANY handle updates ALL views
master.push_y(1.0).unwrap();  // overview and detail see it immediately
overview.push_y(2.0).unwrap(); // master and detail see it immediately

// Generation is shared
assert_eq!(master.generation(), overview.generation());
assert_eq!(master.generation(), detail.generation());

// But each has independent visibility
overview.set_visible(false); // Hidden in overview, visible in others
```

**When to use `share()` vs `clone()`:**

| Scenario | Method | Reason |
|----------|--------|--------|
| Multiple plots showing same live stream | `share()` | Single data source, zero-copy |
| Independent copy for analysis | `clone()` | Isolated mutations |
| Undo/redo snapshots | `clone()` | Point-in-time copy |
| Derived computation | `clone()` | Don't affect source |

### 1.4 Querying Data

```rust
// Bounds (for auto-viewport)
if let Some(viewport) = series.bounds() {
    println!("X: [{}, {}], Y: [{}, {}]", 
        viewport.x.min, viewport.x.max,
        viewport.y.min, viewport.y.max);
}

// Generation (for render cache invalidation)
let gen = series.generation();
// Store last_gen; if gen != last_gen → re-render

// Visibility
if series.is_visible() {
    // Render
}
series.set_visible(false); // Hide without removing

// Direct store access (advanced)
series.with_store(|store| {
    let points = store.points(); // &[Point]
    let point = store.point(100); // Option<Point>
    let range = store.range_by_x(Range::new(0.0, 10.0)); // Index range
    let nearest = store.nearest_index_by_x(5.5); // Hit test
});
```

### 1.5 Series in Plot

```rust
use iced_liveplot::Plot;

let mut plot = Plot::builder()
    .series(&series1)
    .series(&series2)
    .build();

// Access
let series_slice = plot.series();        // &[Series]
let series_mut = plot.series_mut();      // &mut Vec<Series>

// Add dynamically
plot.add_series(&new_series);

// Get pins (markers)
let pins = plot.pins();         // &[Pin]
let pins_mut = plot.pins_mut(); // &mut Vec<Pin>
```

---

## Part 2: DataSource Deep Dive

### 2.1 AppendOnlyData - The Foundation

```rust
use iced_liveplot::datasource::{AppendOnlyData, XMode, AppendError, Point, Viewport, Range};

// Two modes:
// 1. Index: X = implicit index (0, 1, 2...) - FAST, compact
// 2. Explicit: X = user-provided - Flexible, larger

// Creation
let mut indexed = AppendOnlyData::indexed();
let mut explicit = AppendOnlyData::explicit();

// Builders
let data = AppendOnlyData::from_iter_y([1.0, 2.0, 3.0]);           // Index
let data = AppendOnlyData::from_iter_points([Point::new(0.0, 1.0)]); // Explicit
let data = AppendOnlyData::from_explicit_callback(|x| x.sin(), Range::new(0.0, 10.0), 100);

// Appending
indexed.extend_y([4.0, 5.0]).unwrap();           // Index mode only
explicit.extend_points([Point::new(2.0, 3.0)]).unwrap(); // Explicit mode only

// Query
let points = data.points();           // &[Point] - all points
let point = data.point(5);            // Option<Point> - by index
let len = data.len();
let bounds = data.bounds();           // Option<Viewport>

// Range query (O(log n) when monotonic)
let index_range = data.range_by_x(Range::new(1.0, 5.0)); // 1..4
let slice = &data.points()[index_range];

// Nearest neighbor (hit testing)
let idx = data.nearest_index_by_x(2.5); // Option<usize>

// Properties
data.x_mode();        // XMode::Index | Explicit
data.is_monotonic();  // true = binary search available
```

### 2.2 XMode Trade-offs

| Aspect | Index Mode | Explicit Mode |
|--------|------------|---------------|
| Memory | 8 bytes/point (y only) | 16 bytes/point (x+y) |
| Append | O(1) | O(1) |
| Range Query | O(1) index math | O(log n) binary search |
| Monotonicity | Always true | User responsibility |
| Use Case | Uniform sampling, time series | Irregular events, MIDI, audio |

### 2.3 SeriesStore - Store with Decimation

```rust
use iced_liveplot::datasource::{SeriesStore, DecimationScratch};

// Store wraps AppendOnlyData + adds decimation
let mut store = SeriesStore::indexed();          // base_chunk = 64
let mut store = SeriesStore::with_base_chunk(data, 128); // Custom chunk

// Same append methods
store.extend_y(values).unwrap();
store.extend_points(points).unwrap();

// Decimation for rendering
let mut scratch = DecimationScratch::new();
let target_chunk = 64; // ~1 output point per 64 input points

store.decimate(target_chunk, &mut scratch);

// Result in scratch
let decimated = scratch.output();        // &[Point] - decimated
let decimated_mut = scratch.output_mut(); // &mut Vec<Point>

// Buckets for custom rendering
let buckets = scratch.buckets(); // &[MinMax] - min/max per chunk
```

### 2.4 Decimation Algorithm (MinMax)

```
Input:  [P0, P1, P2, P3, P4, P5, P6, P7, P8, P9, ...]  (1000 points)
Chunk:  64 points per output point

Chunk 0: P0-P63   → MinMax { min: (x_min, y_min), max: (x_max, y_max) }
Chunk 1: P64-P127 → MinMax { min: ..., max: ... }
...

Output: [Min0, Max0, Min1, Max1, ...]  (2 points per chunk = 32 points)

Preserves visual envelope! Peaks and valleys maintained.
```

### 2.5 Custom Decimation Level

```rust
fn compute_target_chunk(plot_width_px: f32, max_points: usize) -> usize {
    // Target ~1-2 points per pixel
    let points_per_px = max_points as f32 / plot_width_px;
    (points_per_px.max(1.0) as usize).clamp(1, 1024)
}

// In render loop:
let target_chunk = compute_target_chunk(plot_rect.width(), store.len());
store.decimate(target_chunk, &mut scratch);
```

### 2.6 Performance Characteristics

| Operation | Index Mode | Explicit (Monotonic) | Explicit (Non-Monotonic) |
|-----------|------------|----------------------|--------------------------|
| Append 1 | ~5 ns | ~5 ns | ~5 ns |
| Append 1000 | ~2 μs | ~3 μs | ~3 μs |
| Range Query | O(1) | O(log n) | O(n) |
| Nearest Neighbor | O(1) | O(log n) | O(n) |
| Decimate 1M pts | ~2 ms | ~3 ms | ~3 ms |

---

## Part 3: Best Practices

### 3.1 Streaming Architecture

```rust
// Data thread (high priority, real-time safe)
fn audio_callback(input: &[f32], series: &Series) {
    // Lock-free append (RwLock write)
    let points: Vec<Point> = input.iter().enumerate()
        .map(|(i, &v)| Point::new(i as f64 / 48000.0, v as f64))
        .collect();
    let _ = series.extend_points(points);
}

// UI thread (60 FPS)
fn update_ui(app: &mut App) {
    // Batch UI updates
    if app.series.generation() != app.last_gen {
        app.last_gen = app.series.generation();
        app.program.request_redraw();
    }
}
```

### 3.2 Memory Management

```rust
// For long-running streams, implement rolling window
fn maintain_window(series: &mut Series, max_points: usize) {
    series.with_store_mut(|store| {
        if store.len() > max_points {
            let remove = store.len() - max_points;
            store.drain(0..remove); // Hypothetical - would need implementation
        }
    });
}

// Current workaround: recreate series periodically
if series.generation() > 1_000_000 {
    let new_series = Series::line("New Window");
    // Swap in plot
}
```

### 3.3 Multi-Threaded Data Ingestion

```rust
use std::sync::Arc;
use crossbeam::channel;

// Multiple sources → single series
let series = Series::line("Merged");
let series_clone = series.share();

// Source 1
std::thread::spawn(move || {
    for sample in source1 {
        let _ = series.push_y(sample);
    }
});

// Source 2
std::thread::spawn(move || {
    for sample in source2 {
        let _ = series_clone.push_y(sample);
    }
});

// UI sees merged stream
```

---

## Part 4: Migration Checklist

### From Raw Vectors to Series

| Before | After |
|--------|-------|
| `Vec<f64>` | `Series::line("Name")` + `extend_y()` |
| `Vec<(f64, f64)>` | `Series::scatter("Name")` + `extend_points()` |
| `Vec<Point>` | `Series::from_iter_points()` |
| Manual bounds tracking | `series.bounds()` |
| Manual cache invalidation | `series.generation()` |

### Performance Anti-Patterns

❌ **Don't:**
```rust
// Single appends in hot loop
for v in samples {
    series.push_y(v).unwrap(); // Lock overhead per point!
}

// Clone for every plot
let plot1_series = series.clone();
let plot2_series = series.clone(); // Copies ALL data!
```

✅ **Do:**
```rust
// Batch appends
series.extend_y(samples).unwrap();

// Share for multiple plots
let plot1_series = series.share();
let plot2_series = series.share(); // Zero-copy!
```

---

## Part 5: Future Extensions (Preview)

Based on the Future Directions plan, upcoming APIs:

```rust
// Region-aware series (planned)
series.add_region(DataRegion::midi_note(...));
series.regions_in_range(Range::new(0.0, 10.0));

// Editing actions (planned)
series.insert_points(index, points);
series.delete_points(range);
series.modify_points(changes);

// Undo/redo (planned)
let action = InsertPoints { series: series.id(), points };
dispatcher.execute(action, &mut ctx);
dispatcher.undo(&mut ctx);
```

---

*This guide covers the current stable API. For upcoming features, see [FUTURE_DIRECTIONS.md](FUTURE_DIRECTIONS.md).*