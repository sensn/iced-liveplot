# Taking iced-liveplot to the Next Level

## Executive Summary

This document outlines the architectural gaps between the current `iced-liveplot` (a **plotting widget**) and a **domain-agnostic interactive data visualization framework** capable of powering professional applications across multiple fields.

---

## Current State vs Target State

| Aspect | Current (v0.15) | Target (v1.0+) |
|--------|-----------------|----------------|
| **Primary Use Case** | Scientific plotting, monitoring | DAWs, Scientific, Finance, Industrial, Games |
| **Data Model** | Series + Pins | Series + Regions + Selections + Annotations |
| **Interactions** | Pan, Zoom, Pin, Hover | Full CRUD, Tools, Shortcuts, Gestures |
| **Extensibility** | Theme, Link groups | Plugin system, Custom actions, Custom tools |
| **Editing** | None | Undo/Redo, Copy/Paste, Drag-transform |
| **Domain Support** | Generic | MIDI, Audio, Automation, Financial, SCADA |
| **Performance** | Excellent (1M+ pts) | Excellent + Virtualized + GPU optional |

---

## Critical Missing Pieces (P0 - Must Have)

### 1. Action/Command Framework
**File**: `src/widget/actions.rs` (NEW)

```rust
// Current: Direct mutation in event handlers
fn handle_mouse_up(...) {
    toggle_pin(plot.pins_mut(), pin); // Hardcoded!
}

// Target: Action-based with undo
pub trait PlotAction {
    fn execute(&self, ctx: &mut ActionContext) -> ActionResult;
    fn undo(&self, ctx: &mut ActionContext) -> ActionResult;
}

pub struct AddPin { pin: Pin }
pub struct DeleteRegion { id: RegionId }
pub struct ModifyPoints { series: SeriesId, changes: Vec<(usize, Point)> }
```

**Why**: Enables undo/redo, macros, scripting, collaboration, testing.

### 2. Region System
**File**: `src/region.rs` (NEW)

```rust
// Current: Only Pin (single point marker)
// Target: Rich region hierarchy
pub enum RegionKind {
    Range { x: Range, y: Option<Range> },
    Rectangle { x: Range, y: Range },
    MidiNote { pitch, velocity, channel, start, end },
    AudioRegion { source, start, duration, gain, fades },
    AutomationPoint { param, value, time, curve },
    Marker { label, time, color },
    Loop { start, end },
}
```

**Why**: Pins are insufficient for DAW workflows (notes, clips, automation).

### 3. Selection Model
**File**: `src/selection.rs` (NEW)

```rust
pub struct Selection {
    pins: HashSet<Pin>,
    regions: HashSet<RegionId>,
    points: HashMap<SeriesId, HashSet<usize>>,
    anchor: Option<SelectionAnchor>, // For shift-extend
}
```

**Why**: Multi-select is prerequisite for batch operations, copy/paste, transform tools.

### 4. Keybinding System
**File**: `src/widget/keybindings.rs` (NEW)

```rust
pub enum StandardAction {
    Undo, Redo, Copy, Cut, Paste, Delete,
    SelectAll, ZoomFit, ZoomIn, ZoomOut,
    ToolSelect, ToolPen, ToolEraser, ToolMove,
    SnapToggle, GridToggle,
}
```

**Why**: Professional apps require keyboard-driven workflows.

### 5. Undo/Redo Manager
**File**: `src/history.rs` (NEW)

```rust
pub struct History {
    past: Vec<HistoryGroup>,
    future: Vec<HistoryGroup>,
    max_groups: usize,
}
```

**Why**: Non-negotiable for any editing application.

---

## High Priority (P1 - Should Have)

### 6. Tool System
**File**: `src/widget/tools.rs` (NEW)

```rust
pub enum Tool {
    Select, Pen, Eraser, Move, Scale, Stretch,
    Split, Join, Draw, Line, Curve, Velocity, Quantize,
}
```

**Why**: Mode-based editing (pen vs select vs erase) is standard in DAWs/editors.

### 7. Clipboard & Serialization
**File**: `src/clipboard.rs` (NEW)

```rust
pub enum ClipboardContent {
    Points { series: SeriesId, points: Vec<Point> },
    Regions { regions: Vec<DataRegion> },
    MidiNotes { notes: Vec<MidiNote> },
}
```

**Why**: Copy/paste between tracks, projects, applications.

### 8. Snap/Grid System
**File**: `src/snap.rs` (NEW)

```rust
pub struct SnapConfig {
    enabled: bool,
    grid: Option<GridSpec>,      // Musical: bars/beats/ticks
    time_base: TimeBase,         // Samples, seconds, frames, beats
    threshold_px: f32,
}
```

**Why**: Essential for MIDI editing, audio alignment, precise positioning.

### 9. MIDI Domain Module
**File**: `src/midi/mod.rs` (NEW)

```rust
pub struct MidiTrack {
    notes: Vec<MidiNote>,
    cc_data: HashMap<u8, Vec<(u64, u8)>>,
}

pub fn render_piano_roll(frame, track, transform, range, style);
```

**Why**: Piano roll is the canonical test case for the framework.

### 10. Audio Domain Module
**File**: `src/audio/mod.rs` (NEW)

```rust
pub struct AudioRegion { source, start, duration, gain, fades, pitch, stretch }
pub fn render_waveform(frame, region, transform, range, decimation, style);
```

**Why**: Waveform editing is the other half of DAW core.

### 11. Automation Domain Module
**File**: `src/automation/mod.rs` (NEW)

```rust
pub struct AutomationLane { parameter, range, points, color }
pub enum CurveType { Linear, Exp, Log, SCurve, Step, Bezier }
```

**Why**: Parameter automation is ubiquitous (DAW, scientific, industrial).

---

## Medium Priority (P2 - Nice to Have)

### 12. Plugin System
**File**: `src/plugin.rs` (NEW)

```rust
pub trait PlotPlugin {
    fn register_actions(&self, registry: &mut ActionRegistry);
    fn register_tools(&self, registry: &mut ToolRegistry);
    fn register_region_kinds(&self, registry: &mut RegionKindRegistry);
}
```

**Why**: Allows third-party domains (financial, medical, game dev) without forking.

### 13. Virtualized Rendering
**File**: `src/widget/virtualized.rs` (NEW)

```rust
pub struct VirtualizedRenderer {
    visible_range: Range,
    render_cache: HashMap<SeriesId, CachedCommands>,
}
```

**Why**: Enables massive projects (hours of audio, millions of automation points).

### 14. Project Serialization
**File**: `src/project.rs` (NEW)

```rust
pub struct PlotProject {
    plots: Vec<PlotSnapshot>,
    link_groups: Vec<LinkGroupSnapshot>,
    plugins: Vec<PluginConfig>,
    keybindings: KeyBindings,
    theme: Theme,
}
```

**Why**: Save/load projects, interop with other tools.

### 15. Collaboration Support
**File**: `src/collab.rs` (NEW)

```rust
pub enum CollabMessage {
    Action(Box<dyn PlotAction>),
    Selection(Selection),
    Cursor { user: UserId, position: Point },
}
```

**Why**: Real-time collaborative editing (Figma-style for data).

---

## Low Priority (P3 - Future)

### 16. GPU Acceleration
**File**: `src/gpu.rs` (NEW)

```rust
pub struct GpuRenderer { device, pipelines, buffers }
```

**Why**: Only needed for extreme scale (10M+ points, 4K+ displays).

### 17. AI-Assisted Editing
**File**: `src/ai.rs` (NEW)

```rust
pub fn quantize_notes(notes: &[MidiNote], grid: GridSpec) -> Vec<MidiNote>;
pub fn detect_tempo(audio: &AudioRegion) -> f64;
pub fn suggest_automation_curve(points: &[Point]) -> CurveType;
```

**Why**: Differentiator feature, but high complexity.

---

## Implementation Dependencies

```
Phase 1 (Foundation)
├── 1. Actions Framework
├── 2. Region System
├── 3. Selection Model
├── 4. Keybindings
└── 5. Undo/Redo
     │
     ▼
Phase 2 (Editing)
├── 6. Tool System        ← depends on 1,2,3
├── 7. Clipboard          ← depends on 1,2
├── 8. Snap/Grid          ← depends on 2,3
     │
     ▼
Phase 3 (Domains)
├── 9. MIDI Module        ← depends on 2,6,8
├── 10. Audio Module      ← depends on 2,6
└── 11. Automation Module ← depends on 2,6,8
     │
     ▼
Phase 4 (Ecosystem)
├── 12. Plugin System     ← depends on 1,2,6
├── 13. Virtualized       ← depends on 5,11
├── 14. Project Format    ← depends on 2,7
└── 15. Collaboration     ← depends on 1,5,12
```

---

## API Design Principles

### 1. Progressive Disclosure
```rust
// Simple: Just works
let plot = Plot::builder().series(&s).build();

// Advanced: Opt-in complexity
let plot = Plot::builder()
    .action_dispatcher(dispatcher)
    .tool_system(tool_registry)
    .region_layer(region_layer)
    .build();
```

### 2. Domain-Agnostic Core
```rust
// Core knows NOTHING about MIDI/Audio/Finance
// Domains implement traits:
pub trait RegionRenderer {
    fn render(&self, frame, region, transform, style);
}

pub trait RegionEditor {
    fn hit_test(&self, region, point) -> bool;
    fn create_action(&self, gesture) -> Box<dyn PlotAction>;
}
```

### 3. Composition over Inheritance
```rust
// Plot composes capabilities
struct Plot {
    core: PlotCore,
    actions: Option<ActionDispatcher>,
    tools: Option<ToolSystem>,
    regions: Option<RegionLayer>,
    links: Option<LinkGroup>,
}
```

### 4. Explicit Capabilities
```rust
// Feature flags in Cargo.toml
[features]
default = ["core"]
core = []
actions = ["core"]
regions = ["actions"]
editing = ["regions", "clipboard"]
midi = ["regions", "editing", "midly"]
audio = ["regions", "editing", "symphonia"]
full = ["midi", "audio", "automation", "plugins"]
```

---

## Breaking Changes Required

### 1. `Program` Trait Extension
```rust
// Current
pub trait Program {
    fn with_plot_mut<R>(&self, f: impl FnOnce(&mut Plot) -> R) -> R;
}

// Target
pub trait Program {
    fn with_plot_mut<R>(&self, f: impl FnOnce(&mut Plot) -> R) -> R;
    fn action_dispatcher(&self) -> Option<&ActionDispatcher>;
    fn tool_system(&self) -> Option<&ToolSystem>;
    fn clipboard(&self) -> Option<&Clipboard>;
}
```

### 2. `Message` Enum Expansion
```rust
// Current: ~10 variants
// Target: 50+ variants + extensible via plugin
pub enum Message {
    // Core
    ResetView,
    ViewChanged(Viewport),
    PinAdded(Pin),
    PinRemoved(Pin),
    Hovered(Option<HoverTarget>),
    
    // Actions
    ActionExecuted(Box<dyn PlotAction>),
    ActionUndone,
    ActionRedone,
    
    // Regions
    RegionAdded(RegionId),
    RegionRemoved(RegionId),
    RegionModified(RegionId),
    RegionSelected(HashSet<RegionId>),
    
    // Selection
    SelectionChanged(Selection),
    
    // Tools
    ToolChanged(Tool),
    ToolGestureStarted(ToolGesture),
    ToolGestureUpdated(ToolGesture),
    ToolGestureEnded(ToolGesture),
    
    // Clipboard
    CopyRequested,
    PasteRequested { target: SeriesId },
    CutRequested,
    
    // Domain (plugin-extensible)
    Custom(CustomMessage),
}
```

### 3. `Plot` Structure
```rust
// Current: series, pins, viewport, axes
// Target:
pub struct Plot {
    series: Vec<Series>,
    regions: Vec<DataRegion>,        // NEW
    pins: Vec<Pin>,                  // Legacy, maps to Point regions
    viewport: Option<Viewport>,
    manual_view: Option<Viewport>,
    axes: AxisPair,
    theme: Theme,
    config: PlotViewConfig,
    link_options: PlotLinkOptions,
    // NEW
    action_history: Option<History>,
    tool_state: Option<ToolState>,
    selection: Selection,
}
```

---

## Migration Strategy

### v0.16 - Action Foundation (Non-breaking)
- Add `actions` feature flag
- New `ActionDispatcher` as optional component
- Existing code compiles unchanged

### v0.17 - Regions & Selection (Minor breaking)
- Add `Region` type alongside `Pin`
- `Pin` becomes alias for `RegionKind::Point`
- `Message` enum gains region variants

### v0.18 - Tools & Editing (Breaking)
- `Program` trait adds optional methods
- `Message` enum restructured
- Migration guide + automated tooling

### v1.0 - Stabilization
- All APIs stable
- Plugin system stable
- Domain modules (midi, audio, automation) released

---

## Resource Estimation

| Component | Files | Lines | Weeks | Dependencies |
|-----------|-------|-------|-------|--------------|
| Actions Framework | 3 | ~800 | 2 | None |
| Region System | 2 | ~600 | 2 | Actions |
| Selection Model | 1 | ~300 | 1 | Regions |
| Keybindings | 1 | ~200 | 0.5 | Actions |
| Undo/Redo | 2 | ~400 | 1 | Actions |
| Tool System | 2 | ~600 | 2 | Actions, Regions, Selection |
| Clipboard | 1 | ~300 | 1 | Regions |
| Snap/Grid | 1 | ~400 | 1.5 | Regions |
| MIDI Module | 3 | ~1000 | 3 | Regions, Tools, Snap |
| Audio Module | 3 | ~1000 | 3 | Regions, Tools |
| Automation Module | 2 | ~600 | 2 | Regions, Tools |
| Plugin System | 2 | ~500 | 2 | Actions, Regions |
| Virtualized | 2 | ~600 | 2 | Core |
| Project Format | 1 | ~400 | 1 | Regions, Plugins |
| **TOTAL** | **~26** | **~7,700** | **~22** | |

---

## Testing Strategy

### Unit Tests (Per Module)
```rust
#[test]
fn action_undo_redo() {
    let mut dispatcher = ActionDispatcher::new();
    let mut ctx = test_context();
    
    dispatcher.execute(Box::new(AddPin { pin }), &mut ctx);
    assert_eq!(ctx.plot.pins().len(), 1);
    
    dispatcher.undo(&mut ctx);
    assert_eq!(ctx.plot.pins().len(), 0);
    
    dispatcher.redo(&mut ctx);
    assert_eq!(ctx.plot.pins().len(), 1);
}
```

### Integration Tests
```rust
#[test]
fn midi_piano_roll_workflow() {
    let mut app = TestApp::new();
    
    // Pen tool adds notes
    app.select_tool(Tool::Pen);
    app.click_at(1.0, 60); // C4 at 1s
    app.click_at(2.0, 64); // E4 at 2s
    
    // Select tool moves notes
    app.select_tool(Tool::Select);
    app.drag_select(0.5, 55, 2.5, 70);
    app.drag_move(1.0, 0.0); // Move right 1s
    
    // Quantize
    app.execute_action(QuantizeNotes { grid: GridSpec::sixteenth() });
    
    // Verify
    assert_eq!(app.notes[0].start_time, 1.0); // Quantized
}
```

### Visual Regression Tests
```rust
#[test]
fn render_piano_roll_matches_reference() {
    let frame = render_piano_roll(&track, &transform, &style);
    assert_image_matches(frame, "tests/refs/piano_roll.png", 0.99);
}
```

### Performance Benchmarks
```rust
#[bench]
fn bench_1m_points_render(b: &mut Bencher) {
    let plot = create_plot_with_1m_points();
    b.iter(|| render_frame(&plot, &transform));
    // Target: < 16ms (60 FPS)
}
```

---

## Conclusion

The path from **plotting widget** → **visualization framework** requires:

1. **Foundation first**: Actions, Regions, Selection, Undo (4-5 weeks)
2. **Editing second**: Tools, Clipboard, Snap (3-4 weeks)
3. **Domains third**: MIDI, Audio, Automation (6-8 weeks)
4. **Ecosystem last**: Plugins, Virtualization, Projects (4-5 weeks)

**Total: ~18-22 weeks** for a production-ready v1.0 that can power a DAW, scientific instrument, or trading platform with equal ease.

The current codebase provides an excellent foundation (streaming, decimation, linking, theming). The missing pieces are **interaction architecture** and **domain primitives** - not rendering or data performance.

---

*This document should be updated as implementation progresses. Each phase should have its own detailed spec document.*