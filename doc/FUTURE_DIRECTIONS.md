# Future Directions Implementation Plan

## Vision

Transform `iced-liveplot` from a general-purpose plotting library into a **domain-agnostic interactive data visualization framework** capable of powering professional applications in:
- **DAWs** (Digital Audio Workstations): MIDI piano rolls, audio waveforms, automation lanes
- **Scientific Instruments**: Oscilloscopes, spectrum analyzers, data loggers
- **Financial Trading**: Candlestick charts, order book depth, execution visualization
- **Industrial Control**: SCADA trend displays, alarm histograms, process optimization
- **Game Development**: Profilers, frame-time graphs, memory trackers

---

## Current Capabilities Assessment

| Feature | Status | DAW Readiness |
|---------|--------|---------------|
| Streaming data | ✅ Excellent | ✅ Ready |
| Decimation | ✅ MinMax | ⚠️ Needs LTTB for audio |
| Pan/Zoom | ✅ Smooth | ✅ Ready |
| Pin/Marker | ✅ Basic | ⚠️ Needs region selection |
| Multi-plot linking | ✅ View/Cursor/Brush | ✅ Ready |
| Theming | ✅ Full | ✅ Ready |
| Custom interactions | ❌ None | ❌ Critical gap |
| Data editing (CRUD) | ❌ None | ❌ Critical gap |
| Regions/Selections | ❌ None | ❌ Critical gap |
| Keyboard shortcuts | ❌ None | ❌ Needed |
| Undo/Redo | ❌ None | ❌ Needed |
| Copy/Paste/Cut | ❌ None | ❌ Needed |

---

## Phase 1: Interaction Framework Foundation (Weeks 1-3)

### 1.1 Action/Command Pattern

```rust
// src/widget/actions.rs (NEW)
pub trait PlotAction: Send + Sync {
    fn name(&self) -> &'static str;
    fn execute(&self, ctx: &mut ActionContext) -> ActionResult;
    fn can_undo(&self) -> bool { false }
    fn undo(&self, ctx: &mut ActionContext) -> ActionResult { 
        Err(ActionError::NotReversible) 
    }
}

pub struct ActionContext<'a> {
    pub plot: &'a mut Plot,
    pub state: &'a mut PlotUiState,
    pub config: &'a PlotViewConfig,
    pub clipboard: &'a mut Clipboard,
}

pub enum ActionResult {
    Success,
    Redraw,
    Publish(Message),
    Error(ActionError),
}
```

### 1.2 Built-in Actions

```rust
// Pin/Marker Actions
pub struct AddPin { pub series: SeriesId, pub index: usize }
pub struct RemovePin { pub pin: Pin }
pub struct MovePin { pub pin: Pin, pub new_index: usize }

// Data Editing Actions
pub struct InsertPoints { pub series: SeriesId, pub points: Vec<Point> }
pub struct DeletePoints { pub series: SeriesId, pub range: Range<usize> }
pub struct ModifyPoints { pub series: SeriesId, pub changes: Vec<(usize, Point)> }
pub struct CopyPoints { pub series: SeriesId, pub range: Range<usize> }
pub struct PastePoints { pub series: SeriesId, pub index: usize, pub points: Vec<Point> }

// Region Actions
pub struct AddRegion { pub region: DataRegion }
pub struct RemoveRegion { pub id: RegionId }
pub struct ModifyRegion { pub id: RegionId, pub new_bounds: Range }

// View Actions
pub struct SetViewport { pub viewport: Viewport }
pub struct ResetView
pub struct ZoomToRegion { pub region: DataRegion }
```

### 1.3 Action Dispatcher

```rust
// src/widget/dispatcher.rs (NEW)
pub struct ActionDispatcher {
    history: Vec<Box<dyn PlotAction>>,
    redo_stack: Vec<Box<dyn PlotAction>>,
    max_history: usize,
}

impl ActionDispatcher {
    pub fn execute(&mut self, action: Box<dyn PlotAction>, ctx: &mut ActionContext) -> ActionResult;
    pub fn undo(&mut self, ctx: &mut ActionContext) -> Option<ActionResult>;
    pub fn redo(&mut self, ctx: &mut ActionContext) -> Option<ActionResult>;
    pub fn can_undo(&self) -> bool;
    pub fn can_redo(&self) -> bool;
}
```

### 1.4 Keybinding System

```rust
// src/widget/keybindings.rs (NEW)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StandardAction {
    Undo, Redo,
    Copy, Cut, Paste, Delete,
    SelectAll, DeselectAll,
    ZoomIn, ZoomOut, ZoomFit,
    PanLeft, PanRight, PanUp, PanDown,
    AddPin, RemovePin,
    ToggleGrid, ToggleMinorGrid,
    NextSeries, PrevSeries,
}

pub struct KeyBindings {
    bindings: HashMap<Key, StandardAction>,
    chords: HashMap<Chord, StandardAction>, // e.g., Ctrl+Z
}

impl Default for KeyBindings {
    fn default() -> Self {
        let mut kb = Self::new();
        kb.bind(Key::Z, Modifiers::CTRL, StandardAction::Undo);
        kb.bind(Key::Y, Modifiers::CTRL, StandardAction::Redo);
        kb.bind(Key::C, Modifiers::CTRL, StandardAction::Copy);
        kb.bind(Key::V, Modifiers::CTRL, StandardAction::Paste);
        kb.bind(Key::X, Modifiers::CTRL, StandardAction::Cut);
        kb.bind(Key::Delete, Modifiers::NONE, StandardAction::Delete);
        // ...
        kb
    }
}
```

---

## Phase 2: Region & Selection System (Weeks 3-5)

### 2.1 Data Regions (Domain-Agnostic)

```rust
// src/region.rs (NEW)
#[derive(Debug, Clone, PartialEq)]
pub enum RegionKind {
    // Generic
    Range { x: Range, y: Option<Range> },  // X-range, optional Y-range
    Rectangle { x: Range, y: Range },      // Full rectangle
    Point { x: f64, y: f64 },              // Single point marker
    
    // Audio/MIDI specific (built on generic)
    MidiNote { note: u8, velocity: u8, channel: u8, start: f64, end: f64 },
    AudioRegion { start: f64, end: f64, gain: f64, fade_in: f64, fade_out: f64 },
    AutomationPoint { parameter: String, value: f64, time: f64, curve: CurveType },
    Marker { label: String, time: f64, color: Color },
    Loop { start: f64, end: f64 },
    Punch { start: f64, end: f64 },
}

#[derive(Debug, Clone)]
pub struct DataRegion {
    pub id: RegionId,
    pub kind: RegionKind,
    pub series: Option<SeriesId>,  // Associated series (if any)
    pub style: RegionStyle,
    pub metadata: HashMap<String, Value>, // Extensible
    pub locked: bool,
    pub selected: bool,
}

#[derive(Debug, Clone)]
pub struct RegionStyle {
    pub fill: Option<Color>,
    pub stroke: Option<Color>,
    pub stroke_width: f32,
    pub label: Option<String>,
    pub label_position: LabelPosition,
}
```

### 2.2 Selection Model

```rust
// src/selection.rs (NEW)
#[derive(Debug, Default, Clone)]
pub struct Selection {
    pub pins: HashSet<Pin>,
    pub regions: HashSet<RegionId>,
    pub points: HashMap<SeriesId, HashSet<usize>>, // Series -> point indices
    pub anchor: Option<SelectionAnchor>, // For shift-click extend
}

#[derive(Debug, Clone)]
pub struct SelectionAnchor {
    pub kind: AnchorKind,
    pub position: ScreenPoint,
}

#[derive(Debug, Clone, Copy)]
pub enum AnchorKind {
    Pin(Pin),
    Region(RegionId),
    Point(SeriesId, usize),
}
```

### 2.3 Region Layer in Frame Builder

```rust
// In frame.rs
fn build_regions(render: &mut RenderList, plot: &Plot, state: &PlotUiState) {
    for region in plot.regions() {
        match &region.kind {
            RegionKind::Range { x, y } => {
                // Draw vertical span or rectangle
            }
            RegionKind::MidiNote { note, start, end, .. } => {
                // Draw piano roll note block
            }
            RegionKind::AudioRegion { start, end, gain, .. } => {
                // Draw waveform region with gain envelope
            }
            RegionKind::AutomationPoint { .. } => {
                // Draw automation node
            }
            _ => {}
        }
        
        // Selection highlight
        if state.selection.regions.contains(&region.id) {
            // Draw selection handles
        }
    }
}
```

---

## Phase 3: Domain-Specific Modules (Weeks 5-10)

### 3.1 MIDI Module (`src/midi/`)

```rust
// src/midi/mod.rs (NEW)
pub struct MidiNote {
    pub pitch: u8,        // 0-127
    pub velocity: u8,     // 0-127
    pub channel: u8,      // 0-15
    pub start_tick: u64,  // MIDI ticks
    pub end_tick: u64,
}

impl MidiNote {
    pub fn to_region(&self, ticks_per_beat: u64, tempo: f64) -> DataRegion {
        let start = self.tick_to_seconds(self.start_tick, ticks_per_beat, tempo);
        let end = self.tick_to_seconds(self.end_tick, ticks_per_beat, tempo);
        DataRegion::midi_note(self.pitch, self.velocity, self.channel, start, end)
    }
}

pub struct MidiTrack {
    pub name: String,
    pub channel: u8,
    pub notes: Vec<MidiNote>,
    pub cc_data: HashMap<u8, Vec<(u64, u8)>>, // CC# -> (tick, value)
    pub program: u8,
}

// Piano Roll Rendering
pub fn render_piano_roll(
    frame: &mut canvas::Frame,
    track: &MidiTrack,
    transform: &Transform,
    visible_range: Range,
    style: &PianoRollStyle,
) {
    // Draw piano keys on left
    // Draw note rectangles
    // Draw velocity as color/height
    // Draw CC lanes below
}
```

### 3.2 Audio Module (`src/audio/`)

```rust
// src/audio/mod.rs (NEW)
pub struct AudioRegion {
    pub source: AudioSource,  // File, buffer, generator
    pub start: f64,           // Timeline position
    pub duration: f64,
    pub gain: f64,            // 0.0 - 1.0+
    pub fade_in: f64,         // Seconds
    pub fade_out: f64,
    pub pitch_shift: f64,     // Semitones
    pub time_stretch: f64,    // Ratio
}

pub enum AudioSource {
    File { path: PathBuf, channel: usize },
    Buffer { samples: Arc<Vec<f32>>, sample_rate: u32 },
    Generator { kind: GeneratorKind, params: HashMap<String, f64> },
}

// Waveform Rendering with MinMax Decimation
pub fn render_waveform(
    frame: &mut canvas::Frame,
    region: &AudioRegion,
    transform: &Transform,
    visible_range: Range,
    decimation: &DecimationScratch,
    style: &WaveformStyle,
) {
    // Fetch visible samples
    // Decimate to screen resolution
    // Draw as filled area or outline
    // Overlay gain envelope
}
```

### 3.3 Automation Module (`src/automation/`)

```rust
// src/automation/mod.rs (NEW)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CurveType {
    Linear,
    Exponential,
    Logarithmic,
    SCurve,
    Step,
    Bezier { cp1: Point, cp2: Point },
}

pub struct AutomationLane {
    pub parameter: String,     // e.g., "Volume", "Filter Cutoff"
    pub unit: String,          // "dB", "Hz", "%"
    pub range: Range,          // Min/max value
    pub points: Vec<AutomationPoint>,
    pub color: Color,
}

pub struct AutomationPoint {
    pub time: f64,
    pub value: f64,
    pub curve: CurveType,
    pub selected: bool,
}

impl AutomationLane {
    pub fn value_at(&self, time: f64) -> f64 {
        // Interpolate based on curve type
    }
    
    pub fn insert_point(&mut self, time: f64, value: f64) -> usize {
        // Insert maintaining time order
    }
    
    pub fn to_region(&self) -> Vec<DataRegion> {
        self.points.iter().map(|p| DataRegion::automation_point(
            &self.parameter, p.value, p.time, p.curve
        )).collect()
    }
}
```

---

## Phase 4: Advanced Editing Features (Weeks 10-14)

### 4.1 Multi-Select & Transform Tools

```rust
// src/widget/tools.rs (NEW)
#[derive(Debug, Clone, Copy)]
pub enum Tool {
    Select,           // Click: select, Drag: box select
    Pen,              // Click: add point/region
    Eraser,           // Click: delete point/region
    Move,             // Drag: move selection
    Scale,            // Drag: scale selection (time/pitch)
    Stretch,          // Drag: time-stretch audio
    Split,            // Click: split region at cursor
    Join,             // Drag: join adjacent regions
    Draw,             // Freehand draw automation
    Line,             // Click-drag: straight line automation
    Curve,            // Click-drag: bezier curve automation
}

pub struct ToolContext {
    pub tool: Tool,
    pub modifiers: Modifiers,
    pub snap: SnapConfig,
}

pub struct SnapConfig {
    pub enabled: bool,
    pub grid: Option<GridSpec>,      // Musical grid (bars/beats)
    pub time_base: TimeBase,         // Samples, seconds, frames
    pub threshold_px: f32,           // Snap distance
}
```

### 4.2 Clipboard & Serialization

```rust
// src/clipboard.rs (NEW)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClipboardContent {
    Points { series: SeriesId, points: Vec<Point> },
    Regions { regions: Vec<DataRegion> },
    Pins { pins: Vec<Pin> },
    MidiNotes { notes: Vec<MidiNote> },
    Automation { lanes: Vec<AutomationLane> },
}

pub struct Clipboard {
    content: Option<ClipboardContent>,
    format_version: u32,
}

impl Clipboard {
    pub fn copy(&mut self, content: ClipboardContent);
    pub fn paste(&self) -> Option<&ClipboardContent>;
    pub fn can_paste_into(&self, target: &Plot) -> bool;
    pub fn to_json(&self) -> String;
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error>;
}
```

### 4.3 Undo/Redo with Groups

```rust
// src/history.rs (NEW)
pub struct HistoryGroup {
    actions: Vec<Box<dyn PlotAction>>,
    description: String,
    timestamp: Instant,
}

impl PlotAction for HistoryGroup {
    fn execute(&self, ctx: &mut ActionContext) -> ActionResult {
        for action in &self.actions {
            action.execute(ctx)?;
        }
        Ok(ActionResult::Success)
    }
    
    fn undo(&self, ctx: &mut ActionContext) -> ActionResult {
        for action in self.actions.iter().rev() {
            action.undo(ctx)?;
        }
        Ok(ActionResult::Success)
    }
}

// Auto-grouping for rapid edits (e.g., drag multiple points)
pub struct ActionGrouper {
    current_group: Option<HistoryGroup>,
    group_window: Duration, // e.g., 500ms
    last_action: Instant,
}
```

---

## Phase 5: Performance & Scale (Weeks 14-18)

### 5.1 Virtualized Rendering

```rust
// Only render visible regions + small buffer
pub struct VirtualizedRenderer {
    visible_range: Range,
    render_cache: HashMap<SeriesId, CachedRenderData>,
    dirty_regions: Vec<ScreenRect>,
}

impl VirtualizedRenderer {
    pub fn prepare(&mut self, plot: &Plot, transform: &Transform) {
        // Determine which series/regions are visible
        // Pre-compute decimation for visible range only
        // Cache render commands
    }
    
    pub fn render(&self, frame: &mut canvas::Frame) {
        // Draw only cached visible content
    }
}
```

### 5.2 GPU Acceleration (Optional)

```rust
// wgpu/iced-wgpu integration
pub struct GpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipelines: RenderPipelines,
    buffers: BufferPool,
}

impl GpuRenderer {
    pub fn upload_series(&mut self, series: &Series) -> GpuBufferHandle;
    pub fn draw(&mut self, view: &View, transform: &Transform);
}
```

### 5.3 Background Data Processing

```rust
// Async data pipeline
pub struct DataPipeline {
    rx: mpsc::Receiver<DataCommand>,
    tx: mpsc::Sender<RenderUpdate>,
    workers: Vec<JoinHandle<()>>,
}

pub enum DataCommand {
    Append { series: SeriesId, points: Vec<Point> },
    Decimate { series: SeriesId, target_chunk: usize },
    ComputeBounds { series: SeriesId },
    ComputeStatistics { series: SeriesId, range: Range },
}
```

---

## Phase 6: Ecosystem & Developer Experience (Weeks 18-22)

### 6.1 Plugin System

```rust
// src/plugin.rs (NEW)
pub trait PlotPlugin: Send + Sync {
    fn name(&self) -> &'static str;
    fn version(&self) -> semver::Version;
    fn dependencies(&self) -> Vec<&'static str> { vec![] }
    
    fn register_actions(&self, registry: &mut ActionRegistry);
    fn register_tools(&self, registry: &mut ToolRegistry);
    fn register_region_kinds(&self, registry: &mut RegionKindRegistry);
    fn register_keybindings(&self, bindings: &mut KeyBindings);
    fn on_load(&mut self, context: &PluginContext) -> Result<(), PluginError>;
    fn on_unload(&mut self) -> Result<(), PluginError>;
}

pub struct PluginContext {
    pub plot: Arc<Plot>,
    pub dispatcher: Arc<Mutex<ActionDispatcher>>,
    pub clipboard: Arc<Mutex<Clipboard>>,
    pub config: Arc<RwLock<PlotViewConfig>>,
}
```

### 6.2 Example Plugins

```rust
// plugins/daw_core/src/lib.rs
pub struct DawCorePlugin;

impl PlotPlugin for DawCorePlugin {
    fn register_region_kinds(&self, registry: &mut RegionKindRegistry) {
        registry.register("midi_note", MidiNoteRegion::descriptor());
        registry.register("audio_region", AudioRegionRegion::descriptor());
        registry.register("automation", AutomationRegion::descriptor());
        registry.register("marker", MarkerRegion::descriptor());
        registry.register("loop", LoopRegion::descriptor());
    }
    
    fn register_tools(&self, registry: &mut ToolRegistry) {
        registry.register(Tool::Pen, "MIDI Pen", "Add MIDI notes");
        registry.register(Tool::Velocity, "Velocity Tool", "Edit note velocities");
        registry.register(Tool::Quantize, "Quantize", "Snap to grid");
    }
}
```

### 6.3 Serialization & Project Format

```rust
// Project file format (.icedplot)
#[derive(Serialize, Deserialize)]
pub struct PlotProject {
    pub version: u32,
    pub metadata: ProjectMetadata,
    pub plots: Vec<PlotSnapshot>,
    pub link_groups: Vec<LinkGroupSnapshot>,
    pub plugins: Vec<PluginConfig>,
    pub keybindings: KeyBindings,
    pub theme: Theme,
}

#[derive(Serialize, Deserialize)]
pub struct PlotSnapshot {
    pub id: String,
    pub series: Vec<SeriesSnapshot>,
    pub regions: Vec<DataRegion>,
    pub viewport: Option<Viewport>,
    pub axes: AxisSnapshot,
}
```

---

## Implementation Priority Matrix

| Feature | Effort | Impact | DAW | Science | Finance | Industrial | Priority |
|---------|--------|--------|-----|---------|---------|------------|----------|
| Action Framework | Medium | Critical | ✅ | ✅ | ✅ | ✅ | **P0** |
| Keybindings | Low | High | ✅ | ✅ | ✅ | ✅ | **P0** |
| Region System | High | Critical | ✅ | ✅ | ✅ | ✅ | **P0** |
| Selection Model | Medium | High | ✅ | ✅ | ✅ | ✅ | **P0** |
| Undo/Redo | Medium | High | ✅ | ✅ | ✅ | ✅ | **P0** |
| Copy/Paste | Low | Medium | ✅ | ✅ | ✅ | ✅ | **P1** |
| MIDI Module | High | Critical | ✅ | ❌ | ❌ | ❌ | **P1** |
| Audio Module | High | Critical | ✅ | ❌ | ❌ | ❌ | **P1** |
| Automation Module | Medium | High | ✅ | ✅ | ❌ | ✅ | **P1** |
| Tool System | High | High | ✅ | ✅ | ✅ | ✅ | **P1** |
| Snap/Grid | Medium | High | ✅ | ❌ | ✅ | ✅ | **P1** |
| Plugin System | High | Medium | ✅ | ✅ | ✅ | ✅ | **P2** |
| Virtualized Render | High | Medium | ✅ | ✅ | ✅ | ✅ | **P2** |
| GPU Acceleration | Very High | Low | ⚠️ | ⚠️ | ⚠️ | ⚠️ | **P3** |
| Project Format | Medium | Medium | ✅ | ✅ | ✅ | ✅ | **P2** |

---

## API Evolution Strategy

### Backward Compatibility

1. **Core traits stable**: `Program`, `Series`, `Plot` remain unchanged
2. **New types opt-in**: `Region`, `Action`, `Tool` are additive
3. **Feature flags**: 
   ```toml
   [features]
   default = ["core"]
   core = []
   actions = ["core"]
   regions = ["actions"]
   midi = ["regions", "midly"]
   audio = ["regions", "symphonia"]
   automation = ["regions"]
   plugins = ["actions", "regions"]
   full = ["midi", "audio", "automation", "plugins"]
   ```

### Migration Path

```
v0.15 (current)     → v0.16 (actions)     → v0.17 (regions)     → v1.0 (stable)
├── Core plotting      ├── Action framework  ├── Region types      ├── All stable
├── Basic interactions ├── Keybindings       ├── Selection model   ├── Plugin API
└── Linking            └── Undo/Redo         └── MIDI/Audio/Automation
```

---

## Resource Requirements

| Role | Phase 1-2 | Phase 3-4 | Phase 5-6 |
|------|-----------|-----------|-----------|
| Rust Engineer (Core) | 1.0 FTE | 1.0 FTE | 0.5 FTE |
| Rust Engineer (Domain) | 0.5 FTE | 1.5 FTE | 1.0 FTE |
| UX/Interaction Designer | 0.25 FTE | 0.5 FTE | 0.25 FTE |
| Technical Writer | 0.1 FTE | 0.2 FTE | 0.5 FTE |

**Total**: ~18-22 engineer-weeks for full implementation

---

## Success Metrics

1. **DAW Integration**: Can build a functional piano roll in < 500 lines of app code
2. **Performance**: 60 FPS with 1M+ points, 100+ regions, 10 linked plots
3. **Extensibility**: New domain (e.g., financial candlesticks) in < 200 lines
4. **Developer Experience**: Zero-config default, progressive disclosure of complexity
5. **Ecosystem**: 3+ third-party plugins within 6 months of v1.0

---

## Appendix: Domain-Specific Action Examples

### MIDI Actions
```rust
QuantizeNotes { notes: Vec<RegionId>, grid: GridSpec }
TransposeNotes { notes: Vec<RegionId>, semitones: i8 }
SetVelocity { notes: Vec<RegionId>, velocity: u8 }
SplitNote { note: RegionId, split_tick: u64 }
JoinNotes { notes: Vec<RegionId> }
```

### Audio Actions
```rust
SplitRegion { region: RegionId, position: f64 }
ApplyFade { region: RegionId, fade_in: f64, fade_out: f64 }
TimeStretch { region: RegionId, ratio: f64 }
PitchShift { region: RegionId, semitones: f64 }
ReverseRegion { region: RegionId }
Crossfade { region_a: RegionId, region_b: RegionId, duration: f64 }
```

### Automation Actions
```rust
InsertAutomationPoint { lane: String, time: f64, value: f64 }
DeleteAutomationPoints { lane: String, range: Range<f64> }
SimplifyAutomation { lane: String, tolerance: f64 }
ConvertCurve { points: Vec<RegionId>, curve: CurveType }
```

---

*This document is a living specification. Update as implementation progresses.*