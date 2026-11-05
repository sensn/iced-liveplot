# iced-liveplot ⡈⠑⠢⠔

`iced-liveplot` is a high-performance real-time data plotting library built for the **Iced GUI framework**, designed for append-only telemetry and sensor streams. 

Optimized for live-streaming timeseries data, interactive analytics, and multi-plot synchronization.

It focuses on Iced-native layout, rendering, and interaction for real-time charts.

iced port of https://github.com/Modolet/gpui-liveplot

## Running the examples

```bash
cargo run --bin basic --release
```

```bash
cargo run --bin advanced --release
```

## Features

- **Append-only data model** optimized for streaming workloads.
- **Shared plot-level axes** across all series.
- **Viewport-aware decimation** with multi-level summaries for stable interaction at scale.
- **Interactive pan, zoom, box-zoom, hover readout**, and point pinning.
- **Linked multi-plot interactions** (`x/y` view sync, cursor sync, brush sync, reset sync) perfect for split-screen stack charts.
- **Configurable styles** and dark/light themes.

## Installation

Add this crate to your project:

```toml
[dependencies]
iced-liveplot = "0.1"
```

In Rust code, import it as `iced_liveplot`:

```rust
use iced_liveplot::{Plot, Series, SeriesKind};
```

## Quick Start

```rust
use iced::{Element, Length};
use iced_liveplot::{AxisConfig, Plot, Theme, Series, SeriesKind, widget::plot, Program};

// Inside your Iced application state:
let series = Series::from_iter_y(
    "signal",
    (0..400).map(|i| (i as f64 * 0.03).sin()),
    SeriesKind::Line(Default::default()),
);

let mut my_plot = Plot::builder()
    .id("main_plot")
    .theme(Theme::dark())
    .x_axis(AxisConfig::builder().title("Sample").build())
    .y_axis(AxisConfig::builder().title("Amplitude").build())
    .build();
my_plot.add_series(&series);

let program = Program::new(my_plot);

// Inside your Iced view function:
plot(&self.program)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
```

## Streaming Data

`Series` is append-only. You can keep a shared handle and push new points over time.

- **Implicit X mode:** `Series::line` / `Series::scatter` + `push_y` / `extend_y`
- **Explicit X/Y mode:** `Series::from_iter_points` + `push_point` / `extend_points`

`Plot::add_series` stores a shared series handle, so appends from other handles become visible immediately.

## View Modes

- `View::AutoAll` (default)
- `View::Manual`
- `View::FollowLastN`
- `View::FollowLastNXY`

## Interaction (Iced Backend)

- **Left drag in plot area:** pan
- **Right drag in plot area:** box zoom
- **Mouse wheel in plot area:** zoom both axes around cursor (scroll up zooms in, scroll down zooms out)
- **Mouse wheel on axis area:** zoom single axis (same wheel direction semantics)
- **Left click:** toggle nearest-point pin
- **Double click in plot area / axis:** reset view back to default layout

## Multi-Plot Linking

Use `PlotLinkGroup` and `PlotLinkOptions` to link multiple plot instances together. When plots are linked, hovering or zooming in one plot will automatically move the crosshairs and sync the viewports of all other connected charts in the group seamlessly.

See `bin/advanced.rs` for a complete linked-streaming demo using multiple stacked plots with flexible layout ratios (`Length::FillPortion`).

## Performance Notes

- Line rendering is kept close to `O(plot_width)` through decimation.
- Multi-level summaries speed up zoomed-out views.
- Render caching is keyed by viewport, size, and data generation.

## Limitations

- Append-only workflows are the primary optimization target.
- Only linear axes are currently supported.

## Development

```bash
cargo check
cargo clippy --all-targets
cargo test
```

## License

MIT. See [LICENSE](LICENSE).
