# Implementation Status & Fix Plan for iced-liveplot

## Current Build Status (2026-11-08)
- `cargo check --lib` : **PASSES**
- `cargo build --bin advanced_future_directions --release` : **PASSES**
- `./target/release/advanced_future_directions` : **RUNS** (verified 8s stable launch)

## Major Error Categories FIXED

### 1. Iced 0.15-dev syntax (Element/boxed) — KEY LESSON
In iced 0.15-dev (git master), `From<Widget> for Element` is NOT implemented.
- Use `.boxed()` on widgets when mixing types in a Row/Column (Row::with_children(Vec<Element>))
- Or return the widget DIRECTLY from fn view() -> impl Widget<Message> + use<'_> (see bin/advanced.rs)
- `row![]` / `column![]` macros auto-box via `Widget::_boxed`
- `Row<W>` has NO lifetime param; `Element<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer>`
- iced::Renderer = fallback::Renderer<iced_wgpu::Renderer, iced_tiny_skia::Renderer>

### 2. PlotAction trait objects (FIXED)
- `pub trait PlotAction: Send + Sync + 'static`
- HistoryGroup stores `Vec<Arc<dyn PlotAction>>` (not Box — needs Clone for group)
- ActionDispatcher::execute takes `Arc<A>`; converts to Arc<dyn PlotAction> once then clones
- ActionContext has 5 fields: plot, state, clipboard, history: Option<&mut HistoryGroup>, config

### 3. Keyboard API (FIXED)
- `iced::keyboard::key::Named` (NOT iced::keyboard::Named)
- `Key::Named(Named::Delete)`, `Key::Character("z".into())`
- Equal/Minus/Digit0 are in `Code` enum, not `Named` — use Character("=") etc.
- KeyBindings::lookup (not get_action)

### 4. Color handling (FIXED)
- SerializableColor needs .to_iced() before use as iced::Color
- Color struct literal: `iced::Color { r: 0.2, g: 0.82, b: 0.95, a: 1.0 }`

### 5. Domain APIs (FIXED)
- MidiNote::new(pitch: u8, velocity: u8, channel: u8, start_tick: u64, end_tick: u64)
- MidiSequence has tracks: Vec<MidiTrack>; notes go into track via track.add_note
- MidiNote::tick_to_seconds_static(tick, ticks_per_beat, tempo_bpm)
- AutomationLane::new(parameter, name, unit, range: (f64,f64), default) + insert_point
- CurveType: Linear | Exponential | Logarithmic | SCurve | Step | Bezier (no Sine)
- AudioRegion::new(AudioSource, start, duration); set fields directly (no with_gain)
- AudioSource::File { path, channel } | AudioSource::Buffer { samples, sample_rate, ... }
- SnapConfig { enabled, grid: Option<GridSpec>, time_base, threshold_px, snap_to_regions, snap_region_kinds, gravity }
- GridSpec::musical(division) / seconds / samples / frames (no ::new)
- SnapEngine::config() / config_mut() (field private)
- SnapEngine has no snap_time; use engine.config().grid.as_ref().map(|g| g.snap_time(t, bpm))
- Plot::builder() (not PlotBuilder::new)
- Program has: plot(), with_plot_mut(), theme(), set_theme(), config(), request_redraw()
  (NO dispatcher(), plot_mut(), state_mut(), clipboard_mut(), view())
- widget::plot(&program) returns Canvas — chain .map(Message)
- remove_region(id: RegionId) by value
- StandardAction has: ToolSelect, ToolPen, ToolEraser, ToolMove, ToolScale, ToolStretch, ToggleSnap (no ToolDraw/Line/Curve)

## Remaining minor warnings (non-blocking)
- ~13 unused warnings (unused imports HashMap/HashMap fields, unused muts)
- RenderCommand::Path { path, fill: Option<Color>, stroke: Option<Color>, stroke_width: f32 }

## Verified commands
- `cargo build --bin advanced_future_directions --release` OK (6s)
- `timeout 8 ./target/release/advanced_future_directions` exit 0