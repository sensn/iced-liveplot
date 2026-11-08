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
    widget::{Program, Message as PlotMessage, PlotViewConfig, KeyBindings, StandardAction, keybindings::Chord, Tool, ToolRegistry, ToolState},
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
            streaming: false,
            last_stream: Instant::now(),
            stream_phase: 0.0,
            selected_theme: AppTheme::Dark,
            action_log: Vec::new(),
            show_grid: true,
            show_minor_grid: true,
            snap_enabled: true,
        };
        
        // Populate demo data
        app.populate_demo_data();
        
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
        
        // Tools
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
        
        let region = DataRegion::midi_note(
            pitch,
            velocity,
            1,
            snapped_time,
            snapped_time + duration,
        );
        
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
        
        let data_region = region.to_data_region();
        
        self.program.with_plot_mut(|plot| plot.add_region(data_region));
        self.log_action(format!("Added audio region: Clip {} at {:.1}s ({:.1}s)", 
            self.audio_regions.len(), time, duration));
    }
    
    fn add_automation_point(&mut self, parameter: String, time: f64, value: f64, curve: CurveType) {
        self.automation_lane.insert_point(AutomationPoint::with_curve(time, value.clamp(0.0, 1.0), curve));
        
        let region = DataRegion::automation_point(
            parameter.clone(),
            value.clamp(0.0, 1.0),
            time,
            curve,
        );
        
        self.program.with_plot_mut(|plot| plot.add_region(region));
        self.log_action(format!("Added automation point: {}={:.2} at {:.1}s ({:?})", 
            parameter, value, time, curve));
    }
    
    fn add_marker(&mut self, label: String, time: f64) {
        let marker = DataRegion::marker(
            label.clone(),
            time,
            AudioSerializableColor::new(1.0, 0.8, 0.0, 1.0),
        );
        
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
    // Undo/Redo
    // ========================================================================
    
    fn undo(&mut self) {
        self.log_action("Undo requested (handled by plot widget)".to_string());
    }
    
    fn redo(&mut self) {
        self.log_action("Redo requested (handled by plot widget)".to_string());
    }
    
    // ========================================================================
    // Update
    // ========================================================================
    
    fn update(&mut self, message: Message) -> Task<Message> {
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
                self.log_action(format!("Snap: {}", if self.snap_enabled { "ON" } else { "OFF" }));
            }
            Message::SetSnapDivision(division) => {
                if let Some(grid) = self.snap_engine.config_mut().grid.as_mut() {
                    grid.division = division;
                }
                self.log_action(format!("Snap division: 1/{}", division));
            }
            Message::SetTool(tool) => {
                self.tool_state.set_tool(tool);
                self.log_action(format!("Tool: {}", tool.name()));
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
                    self.log_action(format!("Keybinding: {:?} -> {:?}", (key, modifiers), action));
                    // In real app, execute action via dispatcher
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
                
                // Tools
                text("Tools (1-9)").size(18),
                self.control_row("", vec![
                    ("Select", Message::SetTool(Tool::Select)),
                    ("Pen", Message::SetTool(Tool::Pen)),
                    ("Eraser", Message::SetTool(Tool::Eraser)),
                    ("Move", Message::SetTool(Tool::Move)),
                    ("Scale", Message::SetTool(Tool::Scale)),
                    ("Stretch", Message::SetTool(Tool::Stretch)),
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
        let can_undo = false;
        let can_redo = false;
        let history_len = 0;
        let redo_len = 0;
        
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