//! The iced backend: a [`canvas::Program`] that renders a [`Plot`] and
//! translates mouse events into plot interactions (pan, zoom, pins, hover,
//! box-select, legend toggles).

pub mod actions;
pub mod config;
pub mod constants;
pub mod dispatcher;
pub mod frame;
pub mod geometry;
pub mod history;
pub mod hover;
pub mod keybindings;
pub mod link;
pub mod paint;
pub mod state;
pub mod text;
pub mod tools;

mod events;

use std::cell::RefCell;

use iced::mouse;
use iced::widget::canvas::{self, Cache, Event, Geometry};
use iced::Rectangle;

use crate::plot::Plot;
use crate::style::Theme;
use crate::widget::actions::{ActionDispatcher, Clipboard};

pub use config::PlotViewConfig;
use state::PlotUiState;
// linking
use crate::widget::link::PlotLinkOptions;
use crate::widget::link::{PlotLinkGroup, LinkBinding};
pub use actions::PlotAction;
pub use keybindings::{KeyBindings, StandardAction};
pub use tools::{Tool, ToolRegistry, ToolState};
pub use crate::snap::SnapEngine;

/// A message produced by the plot widget.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    /// Live data tick (from a `time::every` subscription) — refresh and
    /// redraw.
    Tick,
    /// Interaction changed the view or UI state; redraw.
    Redraw,
    /// Toggle the visibility of the series named `name`.
    ToggleSeries(String),
    ResetView, // HIER HINZUFÜGEN
}

/// Editing domain — decides how tools interpret clicks in the plot area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Domain {
    /// MIDI piano-roll editing: pen inserts notes, move drags notes,
    /// Y axis shows note numbers 0-127.
    #[default]
    Midi,
    /// Audio region editing: pen places clip anchors, move drags clips.
    Audio,
    /// Automation lane editing: pen inserts points, draw freehand curves.
    Automation,
    /// Marker editing: pen drops markers at click position.
    Markers,
    /// Loop/punch region editing: pen sets loop start/end.
    Loop,
}

impl Domain {
    pub fn all() -> &'static [Domain] {
        &[Domain::Midi, Domain::Audio, Domain::Automation, Domain::Markers, Domain::Loop]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Domain::Midi => "MIDI",
            Domain::Audio => "Audio",
            Domain::Automation => "Automation",
            Domain::Markers => "Markers",
            Domain::Loop => "Loop",
        }
    }
}

/// Note length selector for the MIDI domain pen tool, in 1/N notes
/// (16 = sixteenth, 8 = eighth, 4 = quarter, 2 = half, 1 = whole).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteLength {
    Sixteenth,
    Eighth,
    Quarter,
    Half,
    Whole,
}

impl NoteLength {
    pub fn all() -> &'static [NoteLength] {
        &[
            NoteLength::Sixteenth,
            NoteLength::Eighth,
            NoteLength::Quarter,
            NoteLength::Half,
            NoteLength::Whole,
        ]
    }

    /// Length in seconds at the given BPM.
    pub fn seconds(&self, bpm: f64) -> f64 {
        let beat = 60.0 / bpm.max(1.0);
        match self {
            NoteLength::Sixteenth => beat / 4.0,
            NoteLength::Eighth => beat / 2.0,
            NoteLength::Quarter => beat,
            NoteLength::Half => beat * 2.0,
            NoteLength::Whole => beat * 4.0,
        }
    }

    /// Length in MIDI ticks at the given ticks-per-beat.
    pub fn ticks(&self, ticks_per_beat: u32) -> u64 {
        (ticks_per_beat as u64) * match self {
            NoteLength::Sixteenth => 1,
            NoteLength::Eighth => 2,
            NoteLength::Quarter => 4,
            NoteLength::Half => 8,
            NoteLength::Whole => 16,
        } / 4
    }

    pub fn label(&self) -> &'static str {
        match self {
            NoteLength::Sixteenth => "1/16",
            NoteLength::Eighth => "1/8",
            NoteLength::Quarter => "1/4",
            NoteLength::Half => "1/2",
            NoteLength::Whole => "1/1",
        }
    }
}

/// X axis display mode for time-based plots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimeAxisMode {
    /// Show seconds (or the plot's native formatter).
    #[default]
    Seconds,
    /// Show MIDI ticks (requires ticks-per-beat).
    Ticks,
    /// Show musical position as `bar.beat` (requires ticks-per-beat).
    Bars,
}

impl TimeAxisMode {
    /// Cycle to the next display mode (for a single toggle button).
    pub fn next(self) -> Self {
        match self {
            TimeAxisMode::Seconds => TimeAxisMode::Ticks,
            TimeAxisMode::Ticks => TimeAxisMode::Bars,
            TimeAxisMode::Bars => TimeAxisMode::Seconds,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            TimeAxisMode::Seconds => "seconds",
            TimeAxisMode::Ticks => "ticks",
            TimeAxisMode::Bars => "bars",
        }
    }
}

/// The plot widget program: owns the plot model and renders it on a canvas.
///
/// The plot lives behind a [`RefCell`] because `canvas::Program::draw` and
/// `update` take `&self`, while the frame builder needs `&mut Plot` to refresh
/// the viewport and hover targets.
pub struct Program {
    plot: RefCell<Plot>,
    theme: RefCell<Theme>,
    config: PlotViewConfig,
    cache: Cache,
    link_binding: Option<LinkBinding>, // HIER ERGÄNZEN: Das Binding für die Synchronisation
    /// Active editing tool state (shared with the app via accessor).
    pub(crate) tool_state: RefCell<ToolState>,
    /// Snap engine used by pen/split tools and rendering of snap guides.
    pub(crate) snap: RefCell<SnapEngine>,
    /// Optional callback invoked when a tool wants to mutate domain data
    /// (insert point, split region, ...). The app decides what to do.
    pub(crate) tool_event: RefCell<Option<ToolEventCallback>>,
    /// Undo/redo history for widget-level edits (pins, viewport, points).
    pub(crate) history: RefCell<crate::widget::actions::ActionDispatcher>,
    /// Clipboard shared by copy/paste actions.
    pub(crate) clipboard: RefCell<crate::widget::actions::Clipboard>,
    /// Active editing domain (MIDI / Audio / Automation / Markers / Loop).
    pub(crate) domain: RefCell<Domain>,
    /// MIDI pen tool note length.
    pub(crate) note_length: RefCell<NoteLength>,
    /// X axis display mode (seconds vs MIDI ticks).
    pub(crate) time_axis: RefCell<TimeAxisMode>,
    /// Ticks per beat used when the axis shows ticks.
    pub(crate) ticks_per_beat: std::cell::Cell<u32>,
    /// Tempo (BPM) for the tick↔seconds scale. 120 BPM = 960 ticks/s.
    pub(crate) tempo_bpm: std::cell::Cell<f64>,
    /// Beats per bar (time signature numerator; e.g. 4 for 4/4).
    pub(crate) beats_per_bar: std::cell::Cell<u32>,
    /// Playhead position in MIDI ticks (0 = start).
    pub(crate) playhead_ticks: std::cell::Cell<u64>,
    /// Transport clock driving the playhead.
    pub(crate) clock: RefCell<crate::clock::Clock>,
}

/// Callback signature for tool-originated domain mutations.
pub type ToolEventCallback = std::rc::Rc<dyn Fn(ToolEvent) -> bool>;

/// Events emitted by tools to the host application.
#[derive(Debug, Clone)]
pub enum ToolEvent {
    /// Insert a data point at the given data-space coordinates.
    InsertPoint { series: crate::series::SeriesId, point: crate::geom::Point },
    /// Insert a MIDI note (MIDI domain pen): pitch from click Y,
    /// start from click X (already snapped), length from NoteLength.
    InsertMidiNote { pitch: u8, velocity: u8, start: f64, duration: f64 },
    /// Insert an audio clip anchor (Audio domain pen).
    InsertAudioClip { start: f64, duration: f64 },
    /// Insert an automation point (Automation domain pen).
    InsertAutomationPoint { time: f64, value: f64 },
    /// Drop a marker (Markers domain pen).
    InsertMarker { time: f64 },
    /// Set the loop region (Loop domain pen: first click = start, second = end
    /// or a drag covering the loop).
    SetLoopRegion { start: f64, end: f64 },
    /// Delete the point nearest to the given coordinates.
    DeletePoint { series: crate::series::SeriesId, index: usize },
    /// Delete the region under the cursor.
    DeleteRegion { region: crate::region::RegionId },
    /// Split the region under the cursor at the given time.
    SplitRegion { region: crate::region::RegionId, time: f64 },
    /// Move a region by a time delta (ticks/seconds depending on domain).
    /// `dy` is the vertical delta in data units (semitones for MIDI,
    /// unitless for automation) — signed, so negative moves DOWN.
    MoveRegion { region: crate::region::RegionId, dt: f64, dy: i8 },
    /// Resize a region edge (start or end).
    ResizeRegion { region: crate::region::RegionId, edge: ResizeEdge, time: f64 },
    /// Move selection by a data-space delta.
    MoveSelection { dx: f64, dy: f64 },
    /// Rubber-band selection finished: all regions intersecting the
    /// data-space rect should be selected.
    BoxSelect { x0: f64, x1: f64, y0: f64, y1: f64 },
    /// A model-level MIDI edit (index based, rebuild-safe).
    Midi(MidiEdit),
    /// Log an informational message (for status display).
    Notify(String),
}

/// A set of MIDI notes a tool wants to edit (group selection).
///
/// Carries the **model indices** of the notes so the host can apply the edit
/// without depending on region ids (which change on every rebuild).
#[derive(Debug, Clone, Default)]
pub struct NoteTargets {
    /// Indices into the host's active track note list.
    pub indices: Vec<usize>,
}

/// A tool edit expressed with model-level MIDI coordinates.
///
/// The widget resolves regions to their data-space coordinates and lets the
/// host translate them back to its own model; carrying ticks/pitches avoids
/// fragile region-id bookkeeping across model rebuilds.
#[derive(Debug, Clone)]
pub enum MidiEdit {
    /// Insert a note at the given ticks/pitch.
    Insert { pitch: u8, velocity: u8, start_tick: u64, end_tick: u64 },
    /// Move the notes identified by `NoteTargets` by a tick/pitch delta.
    Move { targets: NoteTargets, dt_ticks: i64, dy_semitones: i8 },
    /// Resize an edge of the notes identified by `NoteTargets` by a delta.
    ///
    /// `dt_ticks` is the cursor travel in ticks, exactly like `Move`. Applying
    /// the SAME delta to every target keeps the relative lengths of grouped
    /// events intact (Logic Pro semantics) — an absolute edge position would
    /// snap every event to the same border.
    Resize { targets: NoteTargets, edge: ResizeEdge, dt_ticks: i64 },
    /// Delete the notes identified by `NoteTargets`.
    Delete { targets: NoteTargets },
    /// Split the notes at the given tick (only those containing it).
    Split { targets: NoteTargets, tick: u64 },
    /// Select all notes in the active track.
    SelectAll,
    /// Clear the selection.
    ClearSelection,
    /// Quantize the targets (or the whole track when empty) to the grid.
    Quantize { targets: NoteTargets },
    /// Transpose the targets (or the whole track when empty).
    Transpose { targets: NoteTargets, semitones: i8 },
    /// Set the velocity of the targets (or the whole track when empty).
    Velocity { targets: NoteTargets, velocity: u8 },}

/// Which edge of a region is being resized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeEdge {
    Start,
    End,
}

impl Program {
    pub fn new(plot: Plot) -> Self {
        Self {
            plot: RefCell::new(plot),
            theme: RefCell::new(Theme::default()),
            config: PlotViewConfig::default(),
            cache: Cache::new(),
            link_binding: None, // Initial leer
            tool_state: RefCell::new(ToolState::new()),
            snap: RefCell::new(SnapEngine::new(crate::snap::SnapConfig::default())),
            tool_event: RefCell::new(None),
            history: RefCell::new(crate::widget::actions::ActionDispatcher::new()),
            clipboard: RefCell::new(crate::widget::actions::Clipboard::default()),
            domain: RefCell::new(Domain::default()),
            note_length: RefCell::new(NoteLength::Quarter),
            time_axis: RefCell::new(TimeAxisMode::default()),
            ticks_per_beat: std::cell::Cell::new(480),
            tempo_bpm: std::cell::Cell::new(120.0),
            beats_per_bar: std::cell::Cell::new(4),
            playhead_ticks: std::cell::Cell::new(0),
            clock: RefCell::new(crate::clock::Clock::with_ppq(120.0, 480)),
        }
    }

    // ==================== Domain / tool / snap / transport ====================

    /// The active editing domain.
    pub fn domain(&self) -> Domain {
        *self.domain.borrow()
    }

    /// Set the active editing domain. Adjusts the Y axis to the domain
    /// convention (MIDI: note numbers 0-127; Automation: unitless).
    pub fn set_domain(&self, domain: Domain) {
        *self.domain.borrow_mut() = domain;
        let midi_fmt = crate::axis::AxisFormatter::Custom(std::sync::Arc::new(|v| {
            let n = v.round().clamp(0.0, 127.0) as u8;
            format!("{} ({})", n, crate::midi::midi_note_name(n))
        }));
        let auto_fmt = crate::axis::AxisFormatter::Custom(std::sync::Arc::new(|v| {
            format!("{:.2}", v)
        }));
        self.with_plot_mut(|plot| match domain {
            Domain::Midi => plot.y_axis_mut().set_formatter(midi_fmt),
            Domain::Automation => plot.y_axis_mut().set_formatter(auto_fmt),
            _ => {}
        });
        self.apply_time_axis_formatter();
        self.request_redraw();
    }

    /// The MIDI pen note length.
    pub fn note_length(&self) -> NoteLength {
        *self.note_length.borrow()
    }

    /// Set the MIDI pen note length.
    pub fn set_note_length(&self, len: NoteLength) {
        *self.note_length.borrow_mut() = len;
        self.request_redraw();
    }

    /// The X axis display mode.
    pub fn time_axis(&self) -> TimeAxisMode {
        *self.time_axis.borrow()
    }

    /// Switch the X axis between seconds, ticks and bars.
    pub fn set_time_axis(&self, mode: TimeAxisMode) {
        *self.time_axis.borrow_mut() = mode;
        self.apply_time_axis_formatter();
        self.request_redraw();
    }

    /// Read access to the snap engine.
    pub fn snap(&self) -> std::cell::Ref<'_, crate::snap::SnapEngine> {
        self.snap.borrow()
    }

    /// Mutably access the snap engine.
    pub fn with_snap<R>(&self, f: impl FnOnce(&mut crate::snap::SnapEngine) -> R) -> R {
        f(&mut self.snap.borrow_mut())
    }

    /// Replace the snap engine (sync an external config into the widget).
    pub fn set_snap_engine(&self, engine: crate::snap::SnapEngine) {
        *self.snap.borrow_mut() = engine;
        self.request_redraw();
    }

    /// Enable/disable snapping.
    pub fn set_snap_enabled(&self, enabled: bool) {
        self.with_snap(|snap| snap.config_mut().enabled = enabled);
        self.request_redraw();
    }

    /// Register the callback invoked when tools emit domain mutations.
    pub fn set_tool_event_handler(&self, handler: ToolEventCallback) {
        *self.tool_event.borrow_mut() = Some(handler);
    }

    /// Emit a tool event to the host app. Returns whether it was handled.
    pub(crate) fn emit_tool_event(&self, event: ToolEvent) -> bool {
        if let Some(handler) = self.tool_event.borrow().as_ref() {
            handler(event)
        } else {
            false
        }
    }

    /// Start the transport.
    pub fn play(&self) {
        self.clock.borrow_mut().start();
    }

    /// Pause the transport.
    pub fn pause(&self) {
        self.clock.borrow_mut().stop();
    }

    pub fn is_playing(&self) -> bool {
        self.clock.borrow().playing
    }

    /// Set the transport tempo.
    pub fn set_transport_bpm(&self, bpm: f64) {
        self.clock.borrow_mut().set_bpm(bpm);
    }

    /// Advance the transport clock and move the playhead accordingly.
    /// Returns the tick delta since the previous call (0 while stopped).
    pub fn tick_transport(&self) -> u64 {
        let delta = self.clock.borrow_mut().update();
        if delta > 0 {
            let next = self.playhead_ticks.get() + delta;
            self.playhead_ticks.set(next);
            self.request_redraw();
        }
        delta
    }

    /// Read-only access to the transport clock.
    pub fn clock(&self) -> std::cell::Ref<'_, crate::clock::Clock> {
        self.clock.borrow()
    }

    /// Mutably access the transport clock.
    pub fn clock_mut(&self) -> std::cell::RefMut<'_, crate::clock::Clock> {
        self.clock.borrow_mut()
    }

    /// Access the tool state (current tool, active gesture).
    pub fn tool_state(&self) -> std::cell::Ref<'_, ToolState> {
        self.tool_state.borrow()
    }

    /// Mutably access the tool state.
    pub fn with_tool_state<R>(&self, f: impl FnOnce(&mut ToolState) -> R) -> R {
        f(&mut self.tool_state.borrow_mut())
    }

    /// Set the active tool.
    pub fn set_tool(&self, tool: Tool) {
        self.with_tool_state(|ts| ts.set_tool(tool));
        self.request_redraw();
    }

    /// The active tool.
    pub fn current_tool(&self) -> Tool {
        self.tool_state.borrow().current_tool()
    }

    /// Ticks per beat used when the axis shows ticks.
    pub fn ticks_per_beat(&self) -> u32 {
        self.ticks_per_beat.get()
    }

    /// Set ticks per beat.
    pub fn set_ticks_per_beat(&self, tpb: u32) {
        self.ticks_per_beat.set(tpb);
        self.request_redraw();
    }

    /// Tempo driving the tick↔seconds conversion.
    pub fn tempo_bpm(&self) -> f64 {
        self.tempo_bpm.get()
    }

    /// Set the tempo used for tick↔seconds conversions (axis, playhead, tools).
    pub fn set_tempo_bpm(&self, bpm: f64) {
        self.tempo_bpm.set(bpm.clamp(1.0, 1000.0));
        self.request_redraw();
    }

    /// Ticks per second at the current tempo (PPQ × BPM / 60).
    pub(crate) fn ticks_per_second(&self) -> f64 {
        let tpb = self.ticks_per_beat.get().max(1) as f64;
        let bpm = self.tempo_bpm.get().max(1.0);
        tpb * bpm / 60.0
    }

    /// Beats per bar for the musical position axis (time signature numerator).
    pub fn beats_per_bar(&self) -> u32 {
        self.beats_per_bar.get()
    }

    /// Set beats per bar and refresh the X axis formatter if it shows bars.
    pub fn set_beats_per_bar(&self, beats: u32) {
        self.beats_per_bar.set(beats.max(1));
        self.apply_time_axis_formatter();
        self.request_redraw();
    }

    /// Which resize edge (if any) the cursor is currently over.
    ///
    /// Uses the same pixel-based zone detection as the Move tool
    /// ([`crate::widget::tools::TRIM_ZONE_PX`]), so the cursor hint and the
    /// gesture behaviour never disagree. Returns `None` when the pointer is
    /// not over a start/end trim strip (middle of an event → move).
    pub fn resize_edge_at(
        &self,
        state: &crate::widget::state::PlotUiState,
        screen: crate::geom::ScreenPoint,
    ) -> Option<ResizeEdge> {
        let transform = state.transform.clone()?;
        let plot_rect = state.plot_rect?;
        if !crate::widget::geometry::rect_contains(plot_rect, screen) {
            return None;
        }
        let tool = self.current_tool();
        // The dedicated edge tools always trim, so their cursor is always the
        // resize cursor over the matching strip.
        let plot = self.plot.borrow();
        let hit = plot
            .regions()
            .iter()
            .filter(|r| r.visible && !r.locked)
            .filter_map(|r| {
                let zone = crate::widget::tools::TrimZone::zone_for(screen, r, &transform)?;
                match zone {
                    crate::widget::tools::TrimZone::Start => Some((r, ResizeEdge::Start)),
                    crate::widget::tools::TrimZone::End => Some((r, ResizeEdge::End)),
                    crate::widget::tools::TrimZone::Move => None,
                }
            })
            .max_by_key(|(r, _)| r.z_index)?;
        let (_, edge) = hit;
        // For Move/Grab the cursor only changes over the matching strip.
        match tool {
            Tool::Scale if edge == ResizeEdge::End => return None,
            Tool::Stretch if edge == ResizeEdge::Start => return None,
            _ => {}
        }
        Some(edge)
    }

    /// Apply the X axis formatter for the current time-axis mode.
    fn apply_time_axis_formatter(&self) {
        let mode = *self.time_axis.borrow();
        let tpb = self.ticks_per_beat.get().max(1) as f64;
        let ticks_per_sec = self.ticks_per_second();
        let beats_per_bar = self.beats_per_bar.get().max(1) as f64;
        self.with_plot_mut(|plot| match mode {
            TimeAxisMode::Seconds => {
                plot.x_axis_mut().set_formatter(
                    crate::axis::AxisFormatter::Custom(std::sync::Arc::new(|v| format!("{:.1}s", v))),
                );
            }
            TimeAxisMode::Ticks => {
                plot.x_axis_mut().set_formatter(crate::axis::AxisFormatter::Custom(
                    std::sync::Arc::new(move |v| format!("{}", (v * ticks_per_sec).round() as i64)),
                ));
            }
            TimeAxisMode::Bars => {
                plot.x_axis_mut().set_formatter(crate::axis::AxisFormatter::Custom(
                    std::sync::Arc::new(move |v| {
                        let ticks = (v * ticks_per_sec).round() as i64;
                        let beat = ticks as f64 / tpb;
                        let bar = (beat / beats_per_bar).floor() as i64 + 1;
                        let beat_in_bar = beat - (bar as f64 - 1.0) * beats_per_bar;
                        format!("{}.{:03}", bar, (beat_in_bar * 1000.0).round() as i64)
                    }),
                ));
            }
        });
    }

    /// Playhead position in ticks.
    pub fn playhead_ticks(&self) -> u64 {
        self.playhead_ticks.get()
    }

    /// Move the playhead to a tick position.
    pub fn set_playhead_ticks(&self, ticks: u64) {
        self.playhead_ticks.set(ticks);
        self.clock.borrow_mut().seek_ticks(ticks);
        self.request_redraw();
    }

    /// Playhead position in seconds (tempo aware).
    pub fn playhead_seconds(&self) -> f64 {
        self.playhead_ticks.get() as f64 / self.ticks_per_second()
    }

    /// Undo the last widget-level edit. Returns true when something was undone.
    pub fn undo(&self) -> bool {
        let mut ctx = crate::widget::actions::ActionContext {
            plot: &mut self.plot.borrow_mut(),
            state: None,
            clipboard: &mut self.clipboard.borrow_mut(),
            history: None,
            config: &self.config,
        };
        let undone = self.history.borrow_mut().undo(&mut ctx).is_some();
        if undone {
            self.request_redraw();
        }
        undone
    }

    /// Redo the last undone edit. Returns true when something was redone.
    pub fn redo(&self) -> bool {
        let mut ctx = crate::widget::actions::ActionContext {
            plot: &mut self.plot.borrow_mut(),
            state: None,
            clipboard: &mut self.clipboard.borrow_mut(),
            history: None,
            config: &self.config,
        };
        let redone = self.history.borrow_mut().redo(&mut ctx).is_some();
        if redone {
            self.request_redraw();
        }
        redone
    }

    /// Whether an undo is available.
    pub fn can_undo(&self) -> bool {
        self.history.borrow().can_undo()
    }

    /// Whether a redo is available.
    pub fn can_redo(&self) -> bool {
        self.history.borrow().can_redo()
    }
    
    /// Number of undoable entries.
    pub fn history(&self) -> std::cell::Ref<'_, [crate::widget::actions::HistoryGroup]> {
        std::cell::Ref::map(self.history.borrow(), |h| h.history())
    }
    
    /// Number of redoable entries.
    pub fn redo_len(&self) -> usize {
        self.history.borrow().redo_stack().len()
    }

    /// Execute a plot action through the widget history (undoable).
    pub fn execute_action(&self, action: std::sync::Arc<dyn crate::widget::actions::PlotAction>) {
        let mut ctx = crate::widget::actions::ActionContext {
            plot: &mut self.plot.borrow_mut(),
            state: None,
            clipboard: &mut self.clipboard.borrow_mut(),
            history: None,
            config: &self.config,
        };
        self.history.borrow_mut().execute_dyn(action, &mut ctx);
        self.request_redraw();
    }
    
    /// Getter für das Link-Binding (wird in events.rs gebraucht)
    pub fn link_binding(&self) -> Option<&LinkBinding> {
        self.link_binding.as_ref()
    }

    /// Setzt die Konfiguration im Builder-Stil
    pub fn with_config(mut self, config: PlotViewConfig) -> Self {
        self.config = config;
        self
    }

    /// Registriert das Programm in der Link-Gruppe und gibt das verlinkte Programm zurück
    pub fn with_link_group(mut self, group: PlotLinkGroup, options: PlotLinkOptions) -> Self {
    // Da LinkBinding privat ist, erzeugen wir es hier direkt intern, 
    // falls Program es als Feld hält, oder wir nutzen einfach die pub-Methoden der Gruppe:
    let member_id = group.register_member();
    
    // Wenn dein struct Program ein internes link_binding-Feld hat, 
    // nutze den voll-qualifizierten Pfad innerhalb der Lib:
    self.link_binding = Some(crate::widget::link::LinkBinding {
        group,
        member_id,
        options,
    });
    self
}

    /// Immutable access to the plot model.
    pub fn plot(&self) -> std::cell::Ref<'_, Plot> {
        self.plot.borrow()
    }


  pub fn clear_cache(&self) {
        self.cache.clear();
    }
    /// Mutable access to the plot model. Invalidates the geometry cache.
    ///
    /// The closure must not call other `Program` methods that borrow the plot.
    pub fn with_plot_mut<R>(&self, f: impl FnOnce(&mut Plot) -> R) -> R {
        self.cache.clear();
        f(&mut self.plot.borrow_mut())
    }

    /// Immutable access to the plot theme.
    pub fn theme(&self) -> Theme {
        self.theme.borrow().clone()
    }

    /// Replace the plot theme. Invalidates the geometry cache.
    pub fn set_theme(&self, theme: Theme) {
        *self.theme.borrow_mut() = theme;
        self.cache.clear();
    }

    /// The view configuration used when building frames (thresholds for pins
    /// and hover, legend sizing).
    pub fn config(&self) -> &PlotViewConfig {
        &self.config
    }

    /// Mark the geometry as dirty so the next frame repaints.
    pub fn request_redraw(&self) {
        self.cache.clear();
    }
}

impl canvas::Program<Message> for Program {
    type State = PlotUiState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        // 1. Berechne die relative Mausposition innerhalb dieses Widgets
        let local_cursor = match cursor.position_in(bounds) {
            Some(local_pos) => mouse::Cursor::Available(local_pos),
            None => mouse::Cursor::Unavailable,
        };

        // 2. Setze die Bounds für die interne Berechnungslogik auf (0,0) zurück
        let local_bounds = Rectangle {
            x: 0.0,
            y: 0.0,
            ..bounds
        };

        // 3. Übergib die transformierten lokalen Daten an deinen Event-Handler
        events::handle(self, state, event, local_bounds, local_cursor)
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let geometry = self.cache.draw(renderer, bounds.size(), |frame| {
            let mut ui_state = state.clone();
            
            let local_bounds = Rectangle {
                x: 0.0,
                y: 0.0,
                ..bounds
            };

            let list = frame::build_frame(
                self,
                &mut self.plot.borrow_mut(),
                &mut ui_state,
                &self.config,
                local_bounds,
            );
            paint::paint(frame, &list.render);
        });
        vec![geometry]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        // Iced prüft hier standardmäßig mit den echten Fenster-Bounds.
        // Das lassen wir so, damit der Cursor sich nur ändert, wenn die Maus 
        // wirklich über dem jeweiligen Widget schwebt.
        if cursor.is_over(bounds) {
            // Contextual trim cursor: over an event's start/end handle the
            // pointer becomes a horizontal resize (Logic Pro behaviour).
            if let Some(cursor) = cursor.position() {
            let screen = crate::geom::ScreenPoint::new(cursor.x, cursor.y);
                match self.current_tool() {
                    Tool::Grab | Tool::Select => {
                        if self.resize_edge_at(state, screen).is_some() {
                            return mouse::Interaction::ResizingHorizontally;
                        }
                    }
                    Tool::Move | Tool::Scale | Tool::Stretch => {
                        if self.resize_edge_at(state, screen).is_some() {
                            return mouse::Interaction::ResizingHorizontally;
                        }
                    }
                    _ => {}
                }
            }
            match self.current_tool() {
                Tool::Grab => mouse::Interaction::Grab,
                Tool::Select => mouse::Interaction::default(),
                Tool::Pen | Tool::Draw | Tool::Line | Tool::Curve => {
                    mouse::Interaction::Crosshair
                }
                Tool::Eraser => mouse::Interaction::NotAllowed,
                Tool::Move => mouse::Interaction::Move,
                Tool::Split => mouse::Interaction::Crosshair,
                Tool::Scale | Tool::Stretch | Tool::Join | Tool::Velocity
                | Tool::Quantize => mouse::Interaction::Crosshair,
            }
        } else {
            mouse::Interaction::default()
        }
    }
}

/// A canvas widget showing `program`'s plot.
///
/// Returns the concrete [`iced::widget::Canvas`] so callers can chain
/// `.map(...)` to embed the widget into their own message type (like the
/// iced `bezier_tool`/`multitouch` examples do).
/// A canvas widget showing `program`'s plot.
///
/// Returns the concrete [`iced::widget::Canvas`] so callers can chain
/// `.map(...)` to embed the widget into their own message type (like the
/// iced `bezier_tool`/`multitouch` examples do).
pub fn plot<P>(program: P) -> iced::widget::Canvas<P, Message>
where
    P: canvas::Program<Message>,
{
    iced::widget::Canvas::new(program)
        .width(iced::Length::Fill)  // Standardmäßig volle Breite
        .height(iced::Length::Fill) // HIER: Standardmäßig volle Höhe erlauben!
}

