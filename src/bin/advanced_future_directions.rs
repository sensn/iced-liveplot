//! Advanced Future Directions Example
//!
//! This example demonstrates ALL new features implemented in iced-liveplot:
//! - Action/Command Framework with Undo/Redo
//! - Keybindings System
//! - Region System (MIDI, Audio, Automation, Markers, Loops)
//! - Selection Model (multi-selection with modifiers)
//! - Snap/Grid System (musical time)
//! - Tool System (Select, Pen, Eraser, Move, Draw, etc.)
//! - MIDI Domain (Piano Roll)
//! - Audio Domain (Waveform)
//! - Automation Domain (Parameter Curves)
//! - Multi-Plot Linking
//!
//! Run with: cargo run --bin advanced_future_directions

use iced::Widget;
use iced_liveplot::{
    Plot, PlotBuilder, Series, SeriesKind, Theme, Viewport,
    widget::{Program, Message as PlotMessage, PlotViewConfig, KeyBindings, StandardAction, keybindings::Chord, Tool, ToolRegistry, ToolState, ToolEvent},
    geom::Point,
    render::{LineStyle, MarkerStyle, MarkerShape},
    axis::{AxisConfig, AxisFormatter},
    region::{DataRegion, RegionId, RegionKind, RegionStyle, LabelPosition, Range as RegionRange},
    selection::{Selection, SelectionAnchor, AnchorKind},
    snap::{SnapEngine, SnapConfig, GridSpec, TimeBase, SnapType},
    midi::{MidiNote, MidiSequence, MidiTrack, midi_note_name},
    audio::AudioRegion,
    region::{AudioSource, SerializableColor as AudioSerializableColor},
    automation::{AutomationLane, AutomationPoint, AutomationStyle},
    region::{CurveType, SerializableColor as AutoSerializableColor},

    transform::Transform,
    view::Range,
};
use iced::Color;
use iced::{
    widget::{container, column, row, text, button, pick_list, scrollable, Space, rule, checkbox},
    Alignment, Length, Task, Subscription, Element, Color as IcedColor,
    keyboard::{Key, Modifiers},
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::collections::HashMap;

// ============================================================================
// APP STATE
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
enum AppTheme {
    Dark,
    Light,
}

impl std::fmt::Display for AppTheme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppTheme::Dark => f.write_str("Dark"),
            AppTheme::Light => f.write_str("Light"),
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    Plot(PlotMessage),
    // Region editing
    AddMidiNote { pitch: u8, time: f64, duration: f64, velocity: u8 },
    AddAudioRegion { time: f64, duration: f64 },
    AddAutomationPoint { parameter: String, time: f64, value: f64, curve: CurveType },
    AddMarker { label: String, time: f64 },
    SetLoop { start: f64, end: f64 },
    DeleteSelectedRegions,
    // Selection
    SelectAllRegions,
    ClearSelection,
    // Snap/Grid
    ToggleSnap,
    SetSnapDivision(u32),
    // Tools
    SetTool(Tool),
    // Domain selection (sidebar)
    SetDomain(iced_liveplot::widget::Domain),
    // MIDI note length picker (pen tool)
    SetNoteLength(iced_liveplot::widget::NoteLength),
    // X axis mode (ticks vs seconds)
    ToggleTimeAxis,
    // Undo/Redo
    Undo,
    Redo,
    // Theme
    ThemeChanged(AppTheme),
    // Streaming
    Tick,
    SimulateStreaming(bool),
    // Keybinding
    KeyPressed(Key, Modifiers),
}

/// Which series each domain lane is linked to.
#[derive(Debug, Clone, Copy, Default)]
struct LinkedSeries {
    midi: Option<iced_liveplot::series::SeriesId>,
    audio: Option<iced_liveplot::series::SeriesId>,
    automation: Option<iced_liveplot::series::SeriesId>,
    markers: Option<iced_liveplot::series::SeriesId>,
    signal: Option<iced_liveplot::series::SeriesId>,
}

/// A tool interaction worth logging / applying app-side.
#[derive(Debug, Clone)]
enum ToolLog {
    Point(f64, f64),
    RegionDeleted(u64),
    RegionSplit(u64, f64),
    Moved(f64, f64),
    /// MIDI domain pen: insert a note.
    MidiNote(u8, f64, f64),
    /// Audio domain pen: insert a clip.
    AudioClip(f64, f64),
    /// Automation domain pen: insert a point.
    AutoPoint(f64, f64),
    /// Markers domain pen: drop a marker.
    Marker(f64),
    /// Move a region by dt (seconds) and dy (semitones).
    RegionMoved(u64, f64, u8),
    /// Resize a region edge to a new time.
    RegionResized(u64, bool, f64),
    /// Box select: select all regions intersecting the rect.
    BoxSelect(f64, f64, f64, f64),
    Message(String),
}

struct AdvancedApp {
    // Core
    program: Program,
    keybindings: KeyBindings,
    tool_registry: ToolRegistry<'static>,
    tool_state: ToolState,
    snap_engine: SnapEngine,
    selection: Selection,
    
    // Domain data
    midi_sequence: MidiSequence,
    audio_regions: Vec<AudioRegion>,
    automation_lane: AutomationLane,
    markers: Vec<DataRegion>,
    loop_region: Option<DataRegion>,
    
    // Series ids that regions are linked to (legend toggling)
    linked_series: LinkedSeries,
    // Tool events collected from the widget (drained in update)
    pending_tool_events: std::rc::Rc<std::cell::RefCell<Vec<ToolLog>>>,
    
    // State
    streaming: bool,
    last_stream: Instant,
    stream_phase: f64,
    selected_theme: AppTheme,
    action_log: Vec<String>,
    show_grid: bool,
    show_minor_grid: bool,
    snap_enabled: bool,
}

impl AdvancedApp {
    fn new() -> (Self, Task<Message>) {
    
    let signal = Series::from_iter_y(
            "signal",
            (0..400).map(|i| {
                let x = i as f64 * 0.03;
                x.sin()
            }),
            SeriesKind::Line(LineStyle {
                color: iced::Color {
                    r: 0.2,
                    g: 0.75,
                    b: 0.95,
                    a: 1.0,
                },
                width: 2.0,
            }),
        );
        
        
        // Setup keybindings
        let mut keybindings = KeyBindings::default();
        Self::setup_default_bindings(&mut keybindings);
        
        // Setup tools
        let mut tool_registry = ToolRegistry::default();
        let tool_state = ToolState::new();
        
        // Setup snap/grid
        let snap_config = SnapConfig {
            enabled: true,
            grid: Some(GridSpec::musical(16)),
            time_base: TimeBase::Beats,
            threshold_px: 10.0,
            snap_to_regions: true,
            snap_region_kinds: vec![],
            gravity: 0.5,
        };
        let snap_engine = SnapEngine::new(snap_config);
        
        // Custom axis formatter for time
        let time_formatter = AxisFormatter::Custom(Arc::new(|v| format!("{:.1}s", v)));
        
        // Create series for each domain
        let midi_series = Series::scatter("MIDI Notes")
            .with_kind(SeriesKind::Scatter(MarkerStyle {
                color: Color::from_rgb(1.0, 0.8, 0.0),
                size: 6.0,
                shape: MarkerShape::Square,
            }));
        
        let audio_series = Series::line("Audio Regions")
            .with_kind(SeriesKind::Line(LineStyle {
                color: Color::from_rgb(0.2, 0.7, 1.0),
                width: 3.0,
            }));
        
        let automation_series = Series::line("Automation")
            .with_kind(SeriesKind::Line(LineStyle {
                color: Color::from_rgb(1.0, 0.3, 0.6),
                width: 2.0,
            }));
        
        let marker_series = Series::scatter("Markers")
            .with_kind(SeriesKind::Scatter(MarkerStyle {
                color: Color::from_rgb(0.3, 1.0, 0.4),
                size: 8.0,
                shape: MarkerShape::Circle,
            }));
        
        let mut plot = Plot::builder()
        .id("top_plot")
            .theme(Theme::dark())
            .x_axis(AxisConfig::builder()
                .title("Timeline")
                .units("seconds")
                .grid(true)
                .minor_grid(true)
                .formatter(time_formatter)
                .build())
            .y_axis(AxisConfig::builder()
                .title("Pitch / Value")
                .grid(true)
                .zero_line(false)
                .build())
                //plot.add_series(&midi_series);
           // .series(&midi_series)
          //  .series(&audio_series)
            // .series(&signal)
           // .series(&automation_series)
            .series(&marker_series)
            .build();
             plot.add_series(&signal);
             plot.add_series(&midi_series);            
            plot.add_series(&audio_series);
            plot.add_series(&automation_series);
        let program = Program::new(plot);
        
        // Create domain objects
        let mut midi_sequence = MidiSequence::new();
        let mut audio_regions = Vec::new();
        let mut automation_lane = AutomationLane::new(
            "filter_cutoff".to_string(),
            "Filter Cutoff".to_string(),
            "Hz".to_string(),
            (0.0, 1.0),
            0.5,
        );
        let mut markers = Vec::new();
        let mut loop_region = None;
        
        let mut app = Self {

            program,
            keybindings,
            tool_registry,
            tool_state,
            snap_engine,
            selection: Selection::new(),
            midi_sequence,
            audio_regions,
            automation_lane,
            markers,
            loop_region,
            linked_series: LinkedSeries::default(),
            pending_tool_events: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            streaming: false,
            last_stream: Instant::now(),
            stream_phase: 0.0,
            selected_theme: AppTheme::Dark,
            action_log: Vec::new(),
            show_grid: true,
            show_minor_grid: true,
            snap_enabled: true,
        };
        
        // Track which series regions are linked to (for legend toggling)
        let (midi_sid, audio_sid, auto_sid, marker_sid, signal_sid) = {
            let plot = app.program.plot();
            let find = |name: &str| plot.series().iter().find(|s| s.name() == name).map(|s| s.id());
            (
                find("MIDI Notes"),
                find("Audio Regions"),
                find("Automation"),
                find("Markers"),
                find("signal"),
            )
        };
        app.linked_series = LinkedSeries { midi: midi_sid, audio: audio_sid, automation: auto_sid, markers: marker_sid, signal: signal_sid };

        // Wire the tool event handler: tools emit domain mutations; we log
        // them into a shared buffer that `update` drains into real mutations.
        let pending = app.pending_tool_events.clone();
        app.program.set_tool_event_handler(std::rc::Rc::new(move |event| {
            let mut buf = pending.borrow_mut();
            match event {
                ToolEvent::InsertPoint { series: _, point } => {
                    buf.push(ToolLog::Point(point.x, point.y));
                    true
                }
                ToolEvent::InsertMidiNote { pitch, velocity: _, start, duration } => {
                    buf.push(ToolLog::MidiNote(pitch, start, duration));
                    true
                }
                ToolEvent::InsertAudioClip { start, duration } => {
                    buf.push(ToolLog::AudioClip(start, duration));
                    true
                }
                ToolEvent::InsertAutomationPoint { time, value } => {
                    buf.push(ToolLog::AutoPoint(time, value));
                    true
                }
                ToolEvent::InsertMarker { time } => {
                    buf.push(ToolLog::Marker(time));
                    true
                }
                ToolEvent::SetLoopRegion { start, end } => {
                    buf.push(ToolLog::Message(format!("Loop {}-{}s", start, end)));
                    true
                }
                ToolEvent::DeleteRegion { region } => {
                    buf.push(ToolLog::RegionDeleted(region.0));
                    true
                }
                ToolEvent::SplitRegion { region, time } => {
                    buf.push(ToolLog::RegionSplit(region.0, time));
                    true
                }
                ToolEvent::MoveRegion { region, dt, dy } => {
                    buf.push(ToolLog::RegionMoved(region.0, dt, dy));
                    true
                }
                ToolEvent::ResizeRegion { region, edge, time } => {
                    buf.push(ToolLog::RegionResized(region.0, edge == iced_liveplot::widget::ResizeEdge::Start, time));
                    true
                }
                ToolEvent::BoxSelect { x0, x1, y0, y1 } => {
                    buf.push(ToolLog::BoxSelect(x0, x1, y0, y1));
                    true
                }
                ToolEvent::MoveSelection { dx, dy } => {
                    buf.push(ToolLog::Moved(dx, dy));
                    true
                }
                ToolEvent::DeletePoint { series: _, index: _ } => {
                    buf.push(ToolLog::Message("Erase: no point under cursor".into()));
                    true
                }
                ToolEvent::Notify(msg) => {
                    buf.push(ToolLog::Message(msg));
                    true
                }
            }
        }));

        // Populate demo data
        app.populate_demo_data();
        
        // Link each region to its domain series so legend toggles hide
        // both the series and its regions.
        app.link_regions_to_series();
        
        (app, Task::none())
    }
    
    fn setup_default_bindings(keybindings: &mut KeyBindings) {
        use iced::keyboard::{Key, key::Named};
        
        let ch = |c: &str| Key::Character(c.into());
        let named = |n: Named| Key::Named(n);
        
        // Edit
        keybindings.bind(ch("z"), Modifiers::CTRL, StandardAction::Undo);
        keybindings.bind(ch("z"), Modifiers::CTRL | Modifiers::SHIFT, StandardAction::Redo);
        keybindings.bind(ch("y"), Modifiers::CTRL, StandardAction::Redo);
        keybindings.bind(ch("c"), Modifiers::CTRL, StandardAction::Copy);
        keybindings.bind(ch("x"), Modifiers::CTRL, StandardAction::Cut);
        keybindings.bind(ch("v"), Modifiers::CTRL, StandardAction::Paste);
        keybindings.bind(named(Named::Delete), Modifiers::empty(), StandardAction::Delete);
        keybindings.bind(ch("a"), Modifiers::CTRL, StandardAction::SelectAll);
        keybindings.bind(named(Named::Escape), Modifiers::empty(), StandardAction::DeselectAll);
        
        // View
        keybindings.bind(ch("="), Modifiers::CTRL, StandardAction::ZoomIn);
        keybindings.bind(ch("-"), Modifiers::CTRL, StandardAction::ZoomOut);
        keybindings.bind(ch("0"), Modifiers::CTRL, StandardAction::ZoomFit);
        keybindings.bind(ch("r"), Modifiers::CTRL, StandardAction::ResetView);
        keybindings.bind(named(Named::ArrowLeft), Modifiers::CTRL, StandardAction::PanLeft);
        keybindings.bind(named(Named::ArrowRight), Modifiers::CTRL, StandardAction::PanRight);
        keybindings.bind(named(Named::ArrowUp), Modifiers::CTRL, StandardAction::PanUp);
        keybindings.bind(named(Named::ArrowDown), Modifiers::CTRL, StandardAction::PanDown);
        
        // Pins
        keybindings.bind(ch("p"), Modifiers::CTRL, StandardAction::AddPin);
        keybindings.bind(ch("p"), Modifiers::CTRL | Modifiers::SHIFT, StandardAction::RemovePin);
        
        // Grid
        keybindings.bind(ch("g"), Modifiers::CTRL, StandardAction::ToggleGrid);
        keybindings.bind(ch("g"), Modifiers::CTRL | Modifiers::SHIFT, StandardAction::ToggleMinorGrid);
        
        // Tools (Grab is default: key G)
        keybindings.bind(ch("g"), Modifiers::empty(), StandardAction::ToolGrab);
        keybindings.bind(ch("1"), Modifiers::empty(), StandardAction::ToolSelect);
        keybindings.bind(ch("2"), Modifiers::empty(), StandardAction::ToolPen);
        keybindings.bind(ch("3"), Modifiers::empty(), StandardAction::ToolEraser);
        keybindings.bind(ch("4"), Modifiers::empty(), StandardAction::ToolMove);
        keybindings.bind(ch("5"), Modifiers::empty(), StandardAction::ToolScale);
        keybindings.bind(ch("6"), Modifiers::empty(), StandardAction::ToolStretch);
        
        // Snap
        keybindings.bind(ch("s"), Modifiers::CTRL, StandardAction::ToggleSnap);
    }
    
    fn populate_demo_data(&mut self) {
        // Demo MIDI notes (C major scale arpeggio) - times in seconds,
        // converted to ticks (480 ticks/beat @ 120 BPM => 960 ticks/second)
        const TICKS_PER_SECOND: u64 = 960;
        let notes = [
            (60u8, 100u8, 0.0f64, 0.5f64), (64, 90, 0.5, 0.5), (67, 100, 1.0, 0.5), (72, 95, 1.5, 0.5),
            (67, 90, 2.0, 0.5), (64, 100, 2.5, 0.5), (60, 110, 3.0, 1.0),
            (62, 80, 4.0, 0.5), (65, 85, 4.5, 0.5), (69, 80, 5.0, 0.5), (74, 85, 5.5, 1.0),
        ];
        let mut track = MidiTrack::new("Lead".to_string());
        for (pitch, vel, start, dur) in notes {
            let start_tick = (start * TICKS_PER_SECOND as f64) as u64;
            let end_tick = ((start + dur) * TICKS_PER_SECOND as f64) as u64;
            track.add_note(MidiNote::new(pitch, vel, 0, start_tick, end_tick));
        }
        self.midi_sequence.add_track(track);
        
        // Add MIDI regions to plot
        let tempo = self.midi_sequence.tempo;
        let tpq = self.midi_sequence.ticks_per_beat;
        if let Some(first_track) = self.midi_sequence.tracks.first() {
            for note in &first_track.notes {
                let start = MidiNote::tick_to_seconds_static(note.start_tick, tpq, tempo);
                let end = MidiNote::tick_to_seconds_static(note.end_tick, tpq, tempo);
                let region = DataRegion::midi_note(
                    note.pitch,
                    note.velocity,
                    note.channel,
                    start,
                    end,
                );
                self.program.with_plot_mut(|plot| plot.add_region(region));
            }
        }
        
        // Demo audio regions
        for i in 0..4 {
            let start = i as f64 * 2.0;
            let duration = 4.0 - i as f64 * 0.5;
            let mut region = AudioRegion::new(
                AudioSource::File { path: format!("demo_track_{}.wav", i), channel: 0 },
                start,
                duration,
            );
            region.gain = 0.8 - i as f64 * 0.1;
            region.fade_in = 0.01;
            region.fade_out = 0.05;
            self.audio_regions.push(region.clone());
            
            // Add to plot
            let data_region = region.to_data_region();
            self.program.with_plot_mut(|plot| plot.add_region(data_region));
        }
        
        // Demo automation (filter cutoff)
        for i in 0..20 {
            let t = i as f64 * 0.4;
            let v = (t * 0.5).sin() * 0.5 + 0.5;
            let curve = if i % 3 == 0 { CurveType::Exponential } 
                       else if i % 3 == 1 { CurveType::SCurve } 
                       else { CurveType::Linear };
            self.automation_lane.insert_point(AutomationPoint::with_curve(t, v, curve));
        }
        
        // Add automation regions to plot
        for point in &self.automation_lane.points {
            let region = DataRegion::automation_point(
                "filter_cutoff".into(),
                point.value,
                point.time,
                point.curve,
            );
            self.program.with_plot_mut(|plot| plot.add_region(region));
        }
        
        // Demo markers
        for i in 0..5 {
            let time = i as f64 * 8.0;
            let marker = DataRegion::marker(
                format!("Marker {}", i + 1),
                time,
                AudioSerializableColor::new(1.0, 0.8, 0.0, 1.0),
            );
            self.markers.push(marker.clone());
            self.program.with_plot_mut(|plot| plot.add_region(marker));
        }
        
        // Loop region
        self.set_loop(8.0, 16.0);
    }
    
    fn log_action(&mut self, msg: String) {
        self.action_log.push(format!("[{}] {}", 
            chrono::Local::now().format("%H:%M:%S%.3f"), msg));
        if self.action_log.len() > 50 {
            self.action_log.remove(0);
        }
    }
    
    // ========================================================================
    // Region Operations
    // ========================================================================
    
    fn add_midi_note(&mut self, pitch: u8, time: f64, duration: f64, velocity: u8) {
        // Snap time if enabled
        let snapped_time = if self.snap_enabled {
            self.snap_engine
                .config()
                .grid
                .as_ref()
                .map(|g| g.snap_time(time, 120.0))
                .unwrap_or(time)
        } else {
            time
        };
        
        const TICKS_PER_SECOND: u64 = 960;
        let start_tick = (snapped_time * TICKS_PER_SECOND as f64) as u64;
        let end_tick = ((snapped_time + duration) * TICKS_PER_SECOND as f64) as u64;
        let note = MidiNote::new(pitch, velocity, 1, start_tick, end_tick);
        if self.midi_sequence.tracks.is_empty() {
            self.midi_sequence.add_track(MidiTrack::new("Notes".to_string()));
        }
        if let Some(track) = self.midi_sequence.tracks.first_mut() {
            track.add_note(note);
        }
        
        let mut region = DataRegion::midi_note(
            pitch,
            velocity,
            1,
            snapped_time,
            snapped_time + duration,
        );
        region.series = self.linked_series.midi;
        
        self.program.with_plot_mut(|plot| plot.add_region(region));
        self.log_action(format!("Added MIDI note: {} (pitch={}, vel={}, time={:.2}s)", 
            midi_note_name(pitch), pitch, velocity, snapped_time));
    }
    
    fn add_audio_region(&mut self, time: f64, duration: f64) {
        let mut region = AudioRegion::new(
            AudioSource::File { path: format!("clip_{}.wav", self.audio_regions.len()), channel: 0 },
            time,
            duration,
        );
        region.gain = 0.8;
        region.fade_in = 0.01;
        region.fade_out = 0.05;
        self.audio_regions.push(region.clone());
        
        let mut data_region = region.to_data_region();
        data_region.series = self.linked_series.audio;
        
        self.program.with_plot_mut(|plot| plot.add_region(data_region));
        self.log_action(format!("Added audio region: Clip {} at {:.1}s ({:.1}s)", 
            self.audio_regions.len(), time, duration));
    }
    
    fn add_automation_point(&mut self, parameter: String, time: f64, value: f64, curve: CurveType) {
        self.automation_lane.insert_point(AutomationPoint::with_curve(time, value.clamp(0.0, 1.0), curve));
        
        let mut region = DataRegion::automation_point(
            parameter.clone(),
            value.clamp(0.0, 1.0),
            time,
            curve,
        );
        region.series = self.linked_series.automation;
        
        self.program.with_plot_mut(|plot| plot.add_region(region));
        self.log_action(format!("Added automation point: {}={:.2} at {:.1}s ({:?})", 
            parameter, value, time, curve));
    }
    
    fn add_marker(&mut self, label: String, time: f64) {
        let mut marker = DataRegion::marker(
            label.clone(),
            time,
            AudioSerializableColor::new(1.0, 0.8, 0.0, 1.0),
        );
        marker.series = self.linked_series.markers;
        
        self.markers.push(marker.clone());
        self.program.with_plot_mut(|plot| plot.add_region(marker));
        self.log_action(format!("Added marker: {} at {:.1}s", label, time));
    }
    
    fn set_loop(&mut self, start: f64, end: f64) {
        if let Some(old) = self.loop_region.take() {
            self.program.with_plot_mut(|plot| plot.remove_region(old.id));
        }
        
        let region = DataRegion::loop_region(start, end);
        
        self.loop_region = Some(region.clone());
        self.program.with_plot_mut(|plot| plot.add_region(region));
        self.log_action(format!("Set loop: {:.1}s - {:.1}s", start, end));
    }
    
    fn delete_selected_regions(&mut self) {
        let selected_ids: Vec<_> = self.selection.regions.iter().cloned().collect();
        for id in selected_ids.iter().copied() {
            self.program.with_plot_mut(|plot| plot.remove_region(id));
            
            // Remove from domain collections
            if let Some(track) = self.midi_sequence.tracks.first_mut() {
                track.notes.retain(|_n| true); // Region tracking requires id mapping
            }
            self.audio_regions.retain(|r| {
                let region_id = RegionId::new(); // Would need proper tracking
                region_id != id
            });
            self.automation_lane.points.retain(|p| {
                let point_id = RegionId::new(); // Would need proper tracking
                point_id != id
            });
            self.markers.retain(|m| m.id != id);
            if self.loop_region.as_ref().map(|r| r.id) == Some(id) {
                self.loop_region = None;
            }
        }
        let deleted_count = selected_ids.len();
        drop(selected_ids);
        self.selection.regions.clear();
        self.log_action(format!("Deleted {} selected regions", deleted_count));
    }
    
    fn select_all_regions(&mut self) {
        self.program.with_plot_mut(|plot| {
            for region in plot.regions() {
                self.selection.regions.insert(region.id);
            }
        });
        self.log_action(format!("Selected all {} regions", self.selection.regions.len()));
    }
    
    fn clear_selection(&mut self) {
        self.selection.clear();
        self.log_action("Cleared selection".to_string());
    }
    
    // ========================================================================
    // Region/series linking (legend integration)
    // ========================================================================
    
    /// Assign each plot region to its domain series (by kind + timing).
    fn link_regions_to_series(&mut self) {
        let linked = self.linked_series;
        self.program.with_plot_mut(|plot| {
            for region in plot.regions_mut() {
                match &region.kind {
                    RegionKind::MidiNote { .. } => region.series = linked.midi,
                    RegionKind::AudioRegion { .. } => region.series = linked.audio,
                    RegionKind::AutomationPoint { .. } => region.series = linked.automation,
                    RegionKind::Marker { .. } => region.series = linked.markers,
                    _ => {}
                }
            }
        });
    }
    
    // ========================================================================
    // Tool event processing
    // ========================================================================
    
    fn drain_tool_events(&mut self) {
        let events: Vec<ToolLog> = std::mem::take(&mut *self.pending_tool_events.borrow_mut());
        for event in events {
            match event {
                ToolLog::Point(x, y) => {
                    // Generic pen: follow the active domain
                    match self.program.domain() {
                        iced_liveplot::widget::Domain::Midi => {
                            let pitch = y.round().clamp(0.0, 127.0) as u8;
                            let dur = self.program.note_length().seconds(120.0);
                            self.add_midi_note(pitch, x, dur, 100);
                        }
                        iced_liveplot::widget::Domain::Audio => {
                            let dur = self.program.note_length().seconds(120.0) * 4.0;
                            self.add_audio_region(x, dur);
                        }
                        iced_liveplot::widget::Domain::Automation => {
                            let v = y.clamp(0.0, 1.0);
                            let curve = if v > 0.7 { CurveType::Exponential }
                                        else if v < 0.3 { CurveType::SCurve }
                                        else { CurveType::Linear };
                            self.add_automation_point("filter_cutoff".to_string(), x, v, curve);
                        }
                        iced_liveplot::widget::Domain::Markers => {
                            self.add_marker(format!("M {:.2}s", x), x);
                        }
                        iced_liveplot::widget::Domain::Loop => {
                            self.set_loop(x, x + 4.0);
                        }
                    }
                }
                ToolLog::MidiNote(pitch, start, duration) => {
                    self.add_midi_note(pitch, start, duration, 100);
                }
                ToolLog::AudioClip(start, duration) => {
                    self.add_audio_region(start, duration);
                }
                ToolLog::AutoPoint(time, value) => {
                    let v = value.clamp(0.0, 1.0);
                    let curve = if v > 0.7 { CurveType::Exponential }
                                else if v < 0.3 { CurveType::SCurve }
                                else { CurveType::Linear };
                    self.add_automation_point("filter_cutoff".to_string(), time, v, curve);
                }
                ToolLog::Marker(time) => {
                    self.add_marker(format!("M {:.2}s", time), time);
                }
                ToolLog::RegionDeleted(id) => {
                    self.program.with_plot_mut(|plot| plot.remove_region(RegionId(id)));
                    self.markers.retain(|m| m.id.0 != id);
                    if self.loop_region.as_ref().map(|r| r.id.0) == Some(id) {
                        self.loop_region = None;
                    }
                    self.log_action(format!("Eraser: removed region #{}", id));
                }
                ToolLog::RegionSplit(id, time) => {
                    self.split_region(RegionId(id), time);
                }
                ToolLog::RegionMoved(id, dt, dy) => {
                    self.move_region(RegionId(id), dt, dy);
                }
                ToolLog::RegionResized(id, is_start, time) => {
                    self.resize_region(RegionId(id), is_start, time);
                }
                ToolLog::BoxSelect(x0, x1, y0, y1) => {
                    self.box_select(x0, x1, y0, y1);
                }
                ToolLog::Moved(dx, dy) => {
                    self.log_action(format!("Moved selection by ({:.2}, {:.2})", dx, dy));
                }
                ToolLog::Message(msg) => {
                    self.log_action(msg);
                }
            }
        }
    }
    
    /// Split a region at `time` (MIDI notes and audio clips).
    fn split_region(&mut self, id: RegionId, time: f64) {
        let region = self.program.plot().regions().iter().find(|r| r.id == id).cloned();
        let Some(region) = region else { return };
        match &region.kind {
            RegionKind::MidiNote { pitch, velocity, channel, start, end } => {
                if time <= *start || time >= *end {
                    self.log_action("Split: outside note".into());
                    return;
                }
                let (p, v, c, s, e) = (*pitch, *velocity, *channel, *start, *end);
                self.program.with_plot_mut(|plot| plot.remove_region(id));
                let mut a = DataRegion::midi_note(p, v, c, s, time);
                let mut b = DataRegion::midi_note(p, v, c, time, e);
                a.series = region.series;
                b.series = region.series;
                self.program.with_plot_mut(|plot| {
                    plot.add_region(a);
                    plot.add_region(b);
                });
                self.log_action(format!("Split MIDI note at {:.2}s", time));
            }
            RegionKind::AudioRegion { start, duration, .. } => {
                let (s, d) = (*start, *duration);
                if time <= s || time >= s + d {
                    self.log_action("Split: outside clip".into());
                    return;
                }
                self.program.with_plot_mut(|plot| plot.remove_region(id));
                let mut a = AudioRegion::new(AudioSource::File { path: "split_a.wav".into(), channel: 0 }, s, time - s);
                let mut b = AudioRegion::new(AudioSource::File { path: "split_b.wav".into(), channel: 0 }, time, s + d - time);
                let mut ra = a.to_data_region();
                let mut rb = b.to_data_region();
                ra.series = region.series;
                rb.series = region.series;
                let (ra, rb) = (ra, rb);
                self.program.with_plot_mut(|plot| {
                    plot.add_region(ra);
                    plot.add_region(rb);
                });
                self.log_action(format!("Split audio clip at {:.2}s", time));
            }
            _ => self.log_action("Split: unsupported region kind".into()),
        }
    }
    
    /// Move a region by `dt` seconds and `dy` semitones.
    fn move_region(&mut self, id: RegionId, dt: f64, dy: u8) {
        self.program.with_plot_mut(|plot| {
            if let Some(region) = plot.regions_mut().iter_mut().find(|r| r.id == id) {
                match &mut region.kind {
                    RegionKind::MidiNote { start, end, pitch, .. } => {
                        *start = (*start + dt).max(0.0);
                        *end = (*end + dt).max(0.0);
                        *pitch = pitch.saturating_add(dy).min(127).max(0);
                    }
                    RegionKind::AudioRegion { start, .. } => {
                        *start = (*start + dt).max(0.0);
                    }
                    RegionKind::AutomationPoint { time, .. } => {
                        *time = (*time + dt).max(0.0);
                    }
                    RegionKind::Marker { time, .. } => {
                        *time = (*time + dt).max(0.0);
                    }
                    RegionKind::Loop { start, end } => {
                        *start = (*start + dt).max(0.0);
                        *end = (*end + dt).max(0.0);
                    }
                    _ => {}
                }
            }
        });
        self.log_action(format!("Moved region #{} by {:.2}s (+{} st)", id.0, dt, dy));
    }
    
    /// Resize a region edge (start or end) to `time`.
    fn resize_region(&mut self, id: RegionId, is_start: bool, time: f64) {
        self.program.with_plot_mut(|plot| {
            if let Some(region) = plot.regions_mut().iter_mut().find(|r| r.id == id) {
                match &mut region.kind {
                    RegionKind::MidiNote { start, end, .. } => {
                        if is_start {
                            *start = time.min(*end - 0.01).max(0.0);
                        } else {
                            *end = time.max(*start + 0.01);
                        }
                    }
                    RegionKind::AudioRegion { start, duration, .. } => {
                        if is_start {
                            let end = *start + *duration;
                            let new_start = time.min(end - 0.01).max(0.0);
                            *duration = end - new_start;
                            *start = new_start;
                        } else {
                            *duration = (time - *start).max(0.01);
                        }
                    }
                    RegionKind::Loop { start, end } => {
                        if is_start {
                            *start = time.min(*end - 0.01).max(0.0);
                        } else {
                            *end = time.max(*start + 0.01);
                        }
                    }
                    _ => {}
                }
            }
        });
        self.log_action(format!("Resized region #{} {} to {:.2}s", id.0, if is_start { "start" } else { "end" }, time));
    }
    
    /// Select all regions intersecting the data-space box.
    fn box_select(&mut self, x0: f64, x1: f64, y0: f64, y1: f64) {
        let (x_min, x_max) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
        let (y_min, y_max) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
        let mut count = 0;
        self.program.with_plot_mut(|plot| {
            for region in plot.regions_mut() {
                let inside = region.time_range().is_some_and(|t| t.max >= x_min && t.min <= x_max);
                if inside {
                    region.selected = true;
                    self.selection.regions.insert(region.id);
                    count += 1;
                }
            }
        });
        let _ = (y_min, y_max);
        self.log_action(format!("Box select: {} regions in [{:.1}, {:.1}]", count, x_min, x_max));
    }
    
    // ========================================================================
    // Undo/Redo
    // ========================================================================
    
    fn undo(&mut self) {
        if self.program.undo() {
            self.log_action("Undo".to_string());
        } else {
            self.log_action("Undo: nothing to undo".to_string());
        }
    }
    
    fn redo(&mut self) {
        if self.program.redo() {
            self.log_action("Redo".to_string());
        } else {
            self.log_action("Redo: nothing to redo".to_string());
        }
    }
    
    // ========================================================================
    // Update
    // ========================================================================
    
    fn update(&mut self, message: Message) -> Task<Message> {
        // Drain tool events emitted by the widget since last update
        self.drain_tool_events();
        match message {
            Message::Plot(plot_msg) => {
                match plot_msg {
                    PlotMessage::ResetView => {
                        self.log_action("Reset view (double-click)".to_string());
                    }
                    PlotMessage::Redraw => {}
                    _ => {}
                }
            }
            Message::AddMidiNote { pitch, time, duration, velocity } => {
                self.add_midi_note(pitch, time, duration, velocity);
            }
            Message::AddAudioRegion { time, duration } => {
                self.add_audio_region(time, duration);
            }
            Message::AddAutomationPoint { parameter, time, value, curve } => {
                self.add_automation_point(parameter, time, value, curve);
            }
            Message::AddMarker { label, time } => {
                self.add_marker(label, time);
            }
            Message::SetLoop { start, end } => {
                self.set_loop(start, end);
            }
            Message::DeleteSelectedRegions => {
                self.delete_selected_regions();
            }
            Message::SelectAllRegions => {
                self.select_all_regions();
            }
            Message::ClearSelection => {
                self.clear_selection();
            }
            Message::ToggleSnap => {
                self.snap_enabled = !self.snap_enabled;
                self.snap_engine.config_mut().enabled = self.snap_enabled;
                // Keep the widget's snap engine in sync (pen tool snapping)
                let engine = self.snap_engine.clone();
                self.program.set_snap_engine(engine);
                self.log_action(format!("Snap: {}", if self.snap_enabled { "ON" } else { "OFF" }));
            }
            Message::SetSnapDivision(division) => {
                if let Some(grid) = self.snap_engine.config_mut().grid.as_mut() {
                    grid.division = division;
                }
                let engine = self.snap_engine.clone();
                self.program.set_snap_engine(engine);
                self.log_action(format!("Snap division: 1/{}", division));
            }
            Message::SetTool(tool) => {
                self.tool_state.set_tool(tool);
                self.program.set_tool(tool);
                self.log_action(format!("Tool: {} (cursor updated)", tool.name()));
            }
            Message::SetDomain(domain) => {
                self.program.set_domain(domain);
                self.log_action(format!("Domain: {} (Y axis + tool behavior updated)", domain.name()));
            }
            Message::SetNoteLength(len) => {
                self.program.set_note_length(len);
                self.log_action(format!("Note length: {}", len.label()));
            }
            Message::ToggleTimeAxis => {
                let next = match self.program.time_axis() {
                    iced_liveplot::widget::TimeAxisMode::Seconds => iced_liveplot::widget::TimeAxisMode::Ticks,
                    iced_liveplot::widget::TimeAxisMode::Ticks => iced_liveplot::widget::TimeAxisMode::Seconds,
                };
                self.program.set_time_axis(next);
                self.log_action(format!("X axis: {:?}", next));
            }
            Message::Undo => {
                self.undo();
            }
            Message::Redo => {
                self.redo();
            }
            Message::ThemeChanged(theme) => {
                self.selected_theme = theme;
                let iced_theme = match self.selected_theme {
                    AppTheme::Light => Theme::light(),
                    AppTheme::Dark => Theme::dark(),
                };
                self.program.set_theme(iced_theme);
            }
            Message::Tick => {
                if self.streaming {
                    let now = Instant::now();
                    let dt = now.duration_since(self.last_stream).as_secs_f64();
                    self.last_stream = now;
                    self.stream_phase += dt * 0.5;
                    if self.stream_phase > 60.0 {
                        self.stream_phase = 0.0;
                    }
                    let value = self.stream_phase.sin() * 10.0 + 60.0;
                    self.program.with_plot_mut(|plot| {
                        if let Some(series) = plot.series_mut().first_mut() {
                            let _ = series.push_point(Point::new(self.stream_phase, value));
                        }
                    });
                }
                
            }
            Message::SimulateStreaming(enabled) => {
                self.streaming = enabled;
                if enabled {
                    self.last_stream = Instant::now();
                }
                self.log_action(format!("Streaming: {}", if enabled { "ON" } else { "OFF" }));
            }
            Message::KeyPressed(key, modifiers) => {
                if let Some(action) = self.keybindings.lookup(key.clone(), modifiers) {
                    use iced_liveplot::widget::StandardAction;
                    match action {
                        StandardAction::Undo => self.undo(),
                        StandardAction::Redo => self.redo(),
                        StandardAction::ToggleSnap => {
                            self.snap_enabled = !self.snap_enabled;
                            self.program.set_snap_enabled(self.snap_enabled);
                            self.log_action(format!("Snap: {}", if self.snap_enabled { "ON" } else { "OFF" }));
                        }
                        StandardAction::SelectAll => self.select_all_regions(),
                        StandardAction::DeselectAll => self.clear_selection(),
                        StandardAction::Delete => self.delete_selected_regions(),
                        StandardAction::ToolGrab => self.program.set_tool(Tool::Grab),
                        StandardAction::ToolSelect => self.program.set_tool(Tool::Select),
                        StandardAction::ToolPen => self.program.set_tool(Tool::Pen),
                        StandardAction::ToolEraser => self.program.set_tool(Tool::Eraser),
                        StandardAction::ToolMove => self.program.set_tool(Tool::Move),
                        StandardAction::ToolScale => self.program.set_tool(Tool::Scale),
                        StandardAction::ToolStretch => self.program.set_tool(Tool::Stretch),
                        _ => {
                            self.log_action(format!("Key: {:?}", action));
                        }
                    }
                }
            }
        }
                        self.program.request_redraw();
        Task::none()
    }
    
    // ========================================================================
    // View
    // ========================================================================
    
    fn view(&self) -> impl iced::Widget<Message> + use<'_> {
        let plot_widget = iced_liveplot::widget::plot(&self.program).map(Message::Plot);
        
        let controls = self.build_controls();
        let info_panel = self.build_info_panel();
        
        let root = container(
            column![
                container(
                    row![
                        text("iced-liveplot: Advanced Features Demo").size(24),
                        Space::new(),
                        self.theme_picker(),
                    ]
                    .align_y(Alignment::Center)
                )
                .padding(10),
                row![
                    container(plot_widget)
                        .width(Length::FillPortion(3))
                        .height(Length::Fill),
                    column![controls, info_panel]
                        .width(Length::FillPortion(1))
                        .spacing(10)
                        .padding(10),
                ]
                .height(Length::Fill)
                .spacing(0),
            ]
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(IcedColor::from_rgb(0.05, 0.05, 0.06))),
            ..Default::default()
        });
        root
    }
    
    fn build_controls(&self) -> impl iced::Widget<Message> + use<'_> {
        container(
            column![
                // Domain selector
                text("Domain (tool behavior)").size(18),
                self.domain_row(),
                
                // MIDI note length (pen tool)
                self.note_length_row(),
                
                rule::horizontal(2),
                
                // Domain Controls
                text("Domain Controls").size(18),
                self.control_row("MIDI:", vec![
                    ("Add C4", Message::AddMidiNote { pitch: 60, time: 10.0, duration: 0.5, velocity: 100 }),
                    ("Add Chord", Message::AddMidiNote { pitch: 64, time: 11.0, duration: 1.0, velocity: 90 }),
                    ("Add High", Message::AddMidiNote { pitch: 72, time: 12.0, duration: 0.5, velocity: 80 }),
                ]),
                self.control_row("Audio:", vec![
                    ("Add Region", Message::AddAudioRegion { time: 20.0, duration: 4.0 }),
                    ("Add Long", Message::AddAudioRegion { time: 25.0, duration: 8.0 }),
                ]),
                self.control_row("Automation:", vec![
                    ("Linear", Message::AddAutomationPoint { parameter: "Filter".into(), time: 30.0, value: 0.5, curve: CurveType::Linear }),
                    ("Exp", Message::AddAutomationPoint { parameter: "Filter".into(), time: 31.0, value: 0.8, curve: CurveType::Exponential }),
                    ("Sine", Message::AddAutomationPoint { parameter: "Filter".into(), time: 32.0, value: 0.3, curve: CurveType::SCurve }),
                ]),
                self.control_row("Markers:", vec![
                    ("Add Marker", Message::AddMarker { label: "Section".into(), time: 40.0 }),
                ]),
                self.control_row("Loop:", vec![
                    ("Set Loop", Message::SetLoop { start: 8.0, end: 16.0 }),
                    ("Clear Loop", Message::SetLoop { start: 0.0, end: 0.0 }),
                ]),
                
                rule::horizontal(2),
                
                // Selection
                text("Selection").size(18),
                self.control_row("", vec![
                    ("Select All", Message::SelectAllRegions),
                    ("Clear Sel", Message::ClearSelection),
                    ("Delete Sel", Message::DeleteSelectedRegions),
                ]),
                
                rule::horizontal(2),
                
                // Snap/Grid
                text("Snap / Grid").size(18),
                self.control_row("", vec![
                    (if self.snap_enabled { "Snap: ON" } else { "Snap: OFF" }, Message::ToggleSnap),
                    ("Div 4", Message::SetSnapDivision(4)),
                    ("Div 8", Message::SetSnapDivision(8)),
                    ("Div 16", Message::SetSnapDivision(16)),
                    ("Div 32", Message::SetSnapDivision(32)),
                ]),
                
                rule::horizontal(2),
                
                // Tools — Grab is the DEFAULT navigation tool
                text("Tools (1-9, Grab = default)").size(18),
                self.control_row("", vec![
                    (if self.program.current_tool() == Tool::Grab { "[Grab]" } else { "Grab" }, Message::SetTool(Tool::Grab)),
                    ("Select", Message::SetTool(Tool::Select)),
                    ("Pen", Message::SetTool(Tool::Pen)),
                    ("Eraser", Message::SetTool(Tool::Eraser)),
                    ("Move", Message::SetTool(Tool::Move)),
                    ("Scale", Message::SetTool(Tool::Scale)),
                    ("Stretch", Message::SetTool(Tool::Stretch)),
                    ("Split", Message::SetTool(Tool::Split)),
                    ("Draw", Message::SetTool(Tool::Draw)),
                    ("Line", Message::SetTool(Tool::Line)),
                    ("Curve", Message::SetTool(Tool::Curve)),
                ]),
                
                rule::horizontal(2),
                
                // Undo/Redo
                text("History").size(18),
                self.control_row("", vec![
                    ("Undo (Ctrl+Z)", Message::Undo),
                    ("Redo (Ctrl+Shift+Z)", Message::Redo),
                    ("Can Undo", Message::Undo), // Just shows state
                ]),
                
                rule::horizontal(2),
                
                // View
                text("View").size(18),
                self.control_row("", vec![
                    ("Reset View", Message::Plot(PlotMessage::ResetView)),
                    (match self.program.time_axis() {
                        iced_liveplot::widget::TimeAxisMode::Seconds => "X: seconds",
                        iced_liveplot::widget::TimeAxisMode::Ticks => "X: ticks",
                    }, Message::ToggleTimeAxis),
                ]),
                
                rule::horizontal(2),
                
                // Streaming
                text("Streaming").size(18),
                self.control_row("", vec![
                    (if self.streaming { "Stop" } else { "Start" }, Message::SimulateStreaming(!self.streaming)),
                ]),
                text(if self.streaming { "● LIVE" } else { "○ STOPPED" })
                    .color(if self.streaming { IcedColor::from_rgb(1.0, 0.2, 0.2) } 
                           else { IcedColor::from_rgb(0.5, 0.5, 0.5) }).size(12),
                
                rule::horizontal(2),
                
                // Theme
                text("Theme").size(18),
                self.theme_picker(),
                
                rule::horizontal(2),
                
                // Keybindings Help
                text("Keybindings").size(18),
                text("Ctrl+Z: Undo  |  Ctrl+Shift+Z: Redo").size(11).color(IcedColor::from_rgb(0.7, 0.7, 0.8)),
                text("Ctrl+C/V/X: Copy/Paste/Cut  |  Del: Delete").size(11).color(IcedColor::from_rgb(0.7, 0.7, 0.8)),
                text("Ctrl+A: Select All  |  Esc: Deselect").size(11).color(IcedColor::from_rgb(0.7, 0.7, 0.8)),
                text("Ctrl+G: Toggle Grid  |  Ctrl+S: Toggle Snap").size(11).color(IcedColor::from_rgb(0.7, 0.7, 0.8)),
                text("1-9: Tools  |  Ctrl+Arrows: Pan").size(11).color(IcedColor::from_rgb(0.7, 0.7, 0.8)),
            ]
            .spacing(8)
        )
        .padding(10)
        .style(|_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(IcedColor::from_rgb(0.08, 0.08, 0.1))),
            border: iced::Border {
                color: IcedColor::from_rgb(0.2, 0.2, 0.25),
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
    }
    
    /// Sidebar: domain selector row — buttons highlight the active domain.
    fn domain_row(&self) -> iced::Element<'_, Message> {
        let active = self.program.domain();
        let buttons: Vec<(&str, Message)> = iced_liveplot::widget::Domain::all()
            .iter()
            .map(|d| {
                let label = if *d == active {
                    format!("[{}]", d.name())
                } else {
                    d.name().to_string()
                };
                (Box::leak(label.into_boxed_str()) as &'static str, Message::SetDomain(*d))
            })
            .collect();
        self.control_row("", buttons)
    }
    
    /// Sidebar: MIDI note length picker for the pen tool.
    fn note_length_row(&self) -> iced::Element<'_, Message> {
        let active = self.program.note_length();
        let buttons: Vec<(&str, Message)> = iced_liveplot::widget::NoteLength::all()
            .iter()
            .map(|l| {
                let label = if *l == active {
                    format!("[{}]", l.label())
                } else {
                    l.label().to_string()
                };
                (Box::leak(label.into_boxed_str()) as &'static str, Message::SetNoteLength(*l))
            })
            .collect();
        self.control_row("Note len:", buttons)
    }
    
    fn control_row<'a>(&self, label: &'a str, buttons: Vec<(&'a str, Message)>) -> iced::Element<'a, Message> {
        let mut children: Vec<iced::Element<'a, Message>> = Vec::new();
        if !label.is_empty() {
            children.push(text(label).width(100).size(12).boxed());
        }
        for (btn_label, msg) in buttons {
            children.push(button(text(btn_label).size(11)).on_press(msg).boxed());
        }
        iced::widget::Row::with_children(children).spacing(5).boxed()
    }
    
    fn theme_picker(&self) -> impl iced::Widget<Message> + use<'_> {
        let themes = [AppTheme::Dark, AppTheme::Light];
        pick_list(
            Some(self.selected_theme.clone()),
            themes,
            |t| t.to_string(),
        )
        .on_select(|t| Message::ThemeChanged(t.clone()))
        .width(200)
    }
    
    fn build_info_panel(&self) -> impl iced::Widget<Message> + use<'_> {
        let can_undo = self.program.can_undo();
        let can_redo = self.program.can_redo();
        let history_len = self.program.history().len();
        let redo_len = self.program.redo_len();
        
        container(
            column![
                text("Action History").size(18),
                rule::horizontal(1),
                scrollable(
                    column(
                        self.action_log.iter()
                            .rev()
                            .take(20)
                            .map(|log| text(log).size(11).color(IcedColor::from_rgb(0.8, 0.8, 0.85)).boxed())
                            .collect::<Vec<iced::Element<'_, Message>>>()
                    )
                    .spacing(2)
                )
                .height(250),
                rule::horizontal(1),
                text("Statistics").size(14),
                text(format!("MIDI Notes: {}", self.midi_sequence.tracks.first().map(|t| t.notes.len()).unwrap_or(0))).size(12),
                text(format!("Audio Regions: {}", self.audio_regions.len())).size(12),
                text(format!("Auto Points: {}", self.automation_lane.points.len())).size(12),
                text(format!("Markers: {}", self.markers.len())).size(12),
                text(format!("Loop: {}", if self.loop_region.is_some() { "Set" } else { "None" })).size(12),
                text(format!("Streaming: {}", if self.streaming { "ON" } else { "OFF" })).size(12),
                text(format!("Snap: {} (1/{})", if self.snap_enabled { "ON" } else { "OFF" }, self.snap_engine
                    .config()
                    .grid
                    .as_ref()
                    .map(|g| g.division)
                    .unwrap_or(16)))
                .size(12),
                text(format!("Tool: {}", self.tool_state.current_tool().name())).size(12),
                text(format!("Selection: {} regions, {} pins", self.selection.regions.len(), self.selection.pins.len())).size(12),
                rule::horizontal(1),
                text("Undo/Redo Stack").size(14),
                text(format!("Undo: {}  |  Redo: {}", history_len, redo_len)).size(12),
                text(format!("Can Undo: {}  |  Can Redo: {}", can_undo, can_redo)).size(12),
            ]
            .spacing(5)
        )
        .padding(10)
        .style(|_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(IcedColor::from_rgb(0.08, 0.08, 0.1))),
            border: iced::Border {
                color: IcedColor::from_rgb(0.2, 0.2, 0.25),
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
    }
    
    fn subscription(&self) -> Subscription<Message> {
        iced::time::every(Duration::from_millis(16)).map(|_| Message::Tick)
    }
}

// ============================================================================
// MAIN
// ============================================================================

fn main() -> iced::Result {
    iced::application(
        AdvancedApp::new, 
        AdvancedApp::update, 
        AdvancedApp::view
    )
    .subscription(AdvancedApp::subscription)
    .run()
}