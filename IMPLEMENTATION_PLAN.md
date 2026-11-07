# Implementation Plan: gpui-liveplot → iced-liveplot

Port of the GPUI-based live plotting library (`gpui-liveplot/`) to the **iced** GUI
framework, following the idioms of the examples in `iced/examples/` (iced 0.15:
`iced::application(...)`/`iced::run`, `canvas::Program` + `Cache`, `path::Builder`,
`time::every` subscriptions, `pub fn main() -> iced::Result`).

## Key facts from the source scan

- Total source ~2.6k LOC core + ~3.3k LOC GPUI backend + 2 examples (basic, advanced).
- The core is **already backend-agnostic**; only 2 files touch gpui types:
  - `src/style.rs` — uses `gpui::Rgba` for palette colors.
  - `src/render/mod.rs` — uses `gpui::Rgba` in its `Renderer` trait.
- `gpui_backend/` (view/frame/link/hover/paint/state/config/geometry/text/constants)
  is GPUI-specific and will be **replaced** by an iced `canvas`-based backend.
- The datasource module (store + multilevel min/max decimation summaries) is fully
  backend-agnostic → port nearly verbatim.
- Live data updates: in GPUI via `Timer`/tasks + `refresh_viewport`; in iced a
  `time::every` subscription in the app drives `Message::Tick`.

## Iced idioms to follow (from `iced/examples/`)

| Concern | GPUI (source) | iced (port) |
|---|---|---|
| App entry | `App::new()` + `run()` | `iced::application(Update, View)` / `iced::run(update, view)`, `pub fn main() -> iced::Result` |
| Custom widget | `Render`, `paint` fn | `canvas::Program<Message>` with `update()` + `draw()` |
| Retained drawing | element trees | `canvas::Cache` invalidated on state change (`cache.clear()`) |
| Paths | immediate paint calls | `canvas::path::Builder`, `Path`, `Frame::fill/stroke` |
| Colors | `gpui::Rgba` | `iced::Color` |
| Timers / live updates | GPUI `Timer`, async tasks | `subscription` + `iced::time::every` |
| Text in canvas | paint text layer | `canvas::Text` in `frame.fill_text` |
| Geometry primitives | own `Point`/`Rect` | `iced::widget::canvas::{Path, Text}` + `iced::{Point, Rectangle, Vector}` |
| Hover/mouse events | mouse moved handlers | `canvas::Program::update` on `Event::Mouse` |
| Examples structure | `examples/basic.rs`, `examples/advanced.rs` | `src/main.rs` with inline modules (like `game_of_life`, `multitouch`) |

## Module mapping (file by file)

New crate: `iced-liveplot/`

| Source file | Action | Port target |
|---|---|---|---|
| `src/geom.rs` | verbatim (drop `pub(crate)` screen types→keep) | `src/geom.rs` |
| `src/style.rs` | swap `gpui::Rgba` → `iced::Color` | `src/style.rs` |
| `src/view.rs` | verbatim | `src/view.rs` |
| `src/transform.rs` | verbatim | `src/transform.rs` |
| `src/axis.rs` | verbatim (tick generation is pure) | `src/axis.rs` |
| `src/series.rs` | verbatim (swap `gpui::Rgba` if any) | `backend/src/series.rs` |
| `src/render/mod.rs` | adapt: `Renderer` trait + primitives | `src/render/mod.rs` |
| `src/datasource/*` | verbatim | `src/datasource/*` |
| `src/interaction.rs` | verbatim (Pin struct pure) | `src/interaction.rs` |
| `src/plot.rs` | adapt entry points (drop backend refs) | `src/plot.rs` |
| `src/lib.rs` | re-exports | `src/lib.rs` |
| `gpui_backend/*` (9 files) | **replace** | `src/widget/` (new iced backend) |
| `examples/basic.rs` | rewrite as iced app | `src/bin/basic.rs` |
| `examples/advanced.rs` | rewrite as iced app | `src/bin/advanced.rs` |

## New iced backend design (replaces gpui_backend/)

`src/widget/` — an `iced` custom widget `PlotWidget` implementing
`canvas::Program<Message>`:

```
widget/
  mod.rs      — PlotWidget + Message enum (Tick, AddSeries, Zoom, Pan, Pin…)
  cache.rs    — wrapper around canvas::Cache with dirty-tracking
  draw.rs     — axis/grid/series/overlay painting via canvas Frame + Cache
  events.rs   — mouse/keyboard handling in canvas::Program::update
```

- `PlotWidget` holds an inner `crate::plot::Plot` + `canvas::Cache`.
- `draw()` builds all geometry from the current `Transform` + decimated data
  (grid → axes → series → hover/pin overlays), matching the render pipeline
  order of `gpui_backend/paint.rs`.
- `update()` converts iced `Event`s into backend-agnostic interactions (pan,
  box-zoom, wheel-zoom, pin, hover) mirroring `gpui_backend/state.rs`.
- Live data: app-level `subscription` with `time::every(Duration)` → `Message::Tick`
  → `plot.refresh_viewport()` + `cache.clear()`.

## Step-by-step implementation (small chunks)

1. **Scaffold** — `iced-liveplot/Cargo.toml` (iced 0.15 workspace dep), `src/lib.rs` skeleton.
2. **Core port (mostly verbatim)**: `geom.rs`, `view.rs`, `transform.rs`, `axis.rs`,
   `interaction.rs`, `style.rs` (Rgba→Color), `series.rs`.
3. **Datasource**: `datasource/{mod,store,summary}.rs`.
4. **Render primitives**: `render/mod.rs` adapted to iced geometry.
5. **Plot model**: `plot.rs` (drop GPUI-backend-only helpers; keep builder).
6. **Iced widget backend** `src/widget/*` (Program::update + draw + Cache).
7. **Example apps**: `src/bin/basic.rs`, `src/bin/advanced.rs` (iced::application style).
8. **Wire together**: fix lib.rs re-exports, example docs (README), verify build with `cargo build`.
9. **Verify**: `cargo build --all-targets` and `cargo test` for ported unit tests (geom/view/axis/datasource/transform tests).

## Verification

- `cargo build` in `iced-liveplot/` (edition 2021, no external patches needed).
- `cargo test` — ported unit tests from geom/view/transform/axis/datasource modules.
- `cargo run --bin basic` / `--bin advanced` for manual smoke test.

## Out of scope / differences vs GPUI version

- No `gpui_component` theme bridging (`Theme::from_gpui_component_theme`).
- GPUI's immediate-mode paint loop and its separate frame/link/hover modules are
  collapsed into the single `widget/` canvas Program; behavior parity is kept at
  the model level (viewport, pins, zoom/pan, decimation).
