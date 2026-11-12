//! Next-gen 16-track MIDI sequencer built on iced-liveplot.
//!
//! Uses ALL structs of the `midi` module:
//! - [`MidiSequence`] — tempo, time signature, ticks per beat, loop range
//! - [`MidiTrack`] — 16 tracks (name, channel, notes, cc_data, program,
//!   volume, pan, mute, solo)
//! - [`MidiNote`] — pitch / velocity / channel / start_tick / end_tick
//! - [`PianoRollStyle`] — full piano-roll palette (keys, notes, grid)
//!
//! Plus the lib's tool/action/snap/domain systems:
//! - One plot per selected track; the 16 tracks are listed in a sidebar.
//! - MIDI domain: pen inserts notes (length from NoteLength), Move drags
//!   notes, Scale/Stretch resize the edges, box-select groups edits.
//! - Undo/redo + copy/paste go through the widget `ActionDispatcher`.
//! - Snap engine with musical grid (division picker).


use iced::Widget;
use iced::keyboard::{Key, Modifiers};
use iced::widget::{
    Scrollable, button, column, container, row, rule, slider, text, Space,
};
use iced::{Alignment, Color as IcedColor, Element, Length, Subscription, Task};

use iced_liveplot::{
    Plot, Series, SeriesKind, Theme as PlotTheme,
    axis::AxisConfig,
    geom::Point,
    midi::{MidiNote, MidiSequence, MidiTrack, PianoRollStyle, is_black_key, midi_note_name},
    region::{DataRegion, RegionId, RegionKind, SerializableColor},
    render::{MarkerStyle, MarkerShape},
    snap::{GridSpec, SnapConfig, SnapEngine, TimeBase},
    widget::{
        actions::{CopyRegions, PasteRegions, RemoveRegions},
        Domain, Message as PlotMessage, NoteLength, Program, TimeAxisMode, Tool,
    },
};

/// Number of tracks in the sequencer.
const TRACK_COUNT: usize = 16;
/// Default key range shown in the piano roll (C2 .. C7).
const KEY_RANGE: (u8, u8) = (24, 96);
/// Ticks per beat (PPQ).
const PPQ: u64 = 480;
/// Bars visible by default.
const BARS: u32 = 8;
/// MIDI colors per track (deterministic palette).
const TRACK_COLORS: [(f32, f32, f32); TRACK_COUNT] = [
    (1.00, 0.30, 0.30), // 1 red
    (1.00, 0.55, 0.20), // 2 orange
    (1.00, 0.85, 0.20), // 3 yellow
    (0.55, 0.90, 0.25), // 4 lime
    (0.25, 0.85, 0.45), // 5 green
    (0.20, 0.80, 0.70), // 6 teal
    (0.20, 0.65, 0.85), // 7 cyan
    (0.25, 0.45, 0.95), // 8 blue
    (0.50, 0.35, 0.95), // 9 indigo
    (0.70, 0.30, 0.90), // 10 violet
    (0.90, 0.30, 0.75), // 11 magenta
    (0.95, 0.30, 0.50), // 12 pink
    (0.85, 0.40, 0.40), // 13 brick
    (0.70, 0.55, 0.35), // 14 bronze
    (0.45, 0.55, 0.65), // 15 steel
    (0.60, 0.60, 0.65), // 16 grey
];

#[derive(Debug, Clone)]
enum Message {
    /// Select a track in the sidebar (0..15).
    SelectTrack(usize),
    /// Mute/unmute a track.
    ToggleMute(usize),
    /// Solo/unsolo a track.
    ToggleSolo(usize),
    /// Rename a track (via buttons cycling preset names).
    CycleTrackName(usize),
    /// Set the track's MIDI program (0-127).
    SetProgram(usize, i64),
    /// Set the track's volume (0-127).
    SetVolume(usize, i64),
    /// Set the track's pan (0-127).
    SetPan(usize, i64),
    /// Select the tool.
    SetTool(Tool),
    /// Select the pen note length.
    SetNoteLength(NoteLength),
    /// Select the editing domain.
    SetDomain(Domain),
    /// Set the snap division (per bar: 4=quarter, 8=eighth, 16=16th...).
    SetSnapDivision(u32),
    /// Toggle snapping.
    ToggleSnap,
    /// Undo.
    Undo,
    /// Redo.
    Redo,
    /// Copy the selection into the clipboard.
    Copy,
    /// Cut the selection.
    Cut,
    /// Paste the clipboard.
    Paste,
    /// Delete the selected regions.
    DeleteSelection,
    /// Clear the selection.
    ClearSelection,
    /// Select all notes of the active track.
    SelectAllTrack,
    /// Quantize the active track to the snap grid.
    QuantizeTrack,
    /// Transpose the active track by N semitones.
    TransposeTrack(i8),
    /// Set the velocity of selected notes (slider, clamped to u8 on use).
    SetVelocity(i64),
    /// Set the BPM (slider).
    SetTempo(f64),
    /// Set the loop range to the whole sequence / none.
    ToggleLoop,
    /// Zoom by N bars (negative = zoom out).
    ZoomBars(i32),
    /// Toggle X axis ticks/seconds.
    ToggleTimeAxis,
    /// Cycle the musical time signature through common (and not so common)
    /// divisions: 4/4, 3/4, 6/8, 7/8, 9/8, 13/8, 5/4, 7/4.
    CycleTimeSignature,
    /// Toggle showing note names.
    ToggleNoteNames,
    /// Toggle transport play/pause.
    TogglePlay,
    /// Move the playhead to the start.
    SeekStart,
    /// A key was pressed.
    KeyPressed(Key, Modifiers),
    /// Tick for the playhead animation.
    Tick,
    /// A message from the plot widget.
    Plot(PlotMessage),
}

/// Seconds → MIDI ticks at `tempo` BPM with PPQ resolution.
fn sec_to_ticks(seconds: f64, tempo: f64) -> u64 {
    let beats = seconds * tempo / 60.0;
    (beats * PPQ as f64).round() as u64
}

struct SequencerApp {
    /// The MIDI model: 16 tracks, tempo, loop.
    sequence: MidiSequence,
    /// Index of the visible/edited track.
    active: usize,
    /// The plot widget program for the active track.
    program: Program,
    /// Piano roll styling.
    roll_style: PianoRollStyle,
    /// Snap engine (shared with the widget).
    snap_engine: SnapEngine,
    /// Selection (region ids).
    selection: Vec<RegionId>,
    /// Active tool.
    tool: Tool,
    /// Active domain.
    domain: Domain,
    /// Pen note length.
    note_length: NoteLength,
    /// Snap enabled.
    snap_enabled: bool,
    /// Snap division per bar.
    snap_division: u32,
    /// Tempo BPM (mirrors sequence.tempo).
    tempo: f64,
    /// Bars visible.
    bars: u32,
    /// Show note names.
    show_names: bool,
    /// X axis mode.
    time_axis: TimeAxisMode,
    /// Playhead position in seconds (advanced by Tick).
    playhead: f64,
    /// Playing state.
    playing: bool,
    /// Log line for the status bar.
    status: String,
    /// Rebuild counter: bumped whenever regions change so view() refreshes.
    revision: u64,
    /// Tool events emitted by the widget (drained in `update`).
    pending_tool_events: std::rc::Rc<std::cell::RefCell<Vec<ToolEvent>>>,
    /// Model-level note clipboard (cross-track paste source).
    clipboard: Vec<MidiNote>,
}

/// Tool interactions that the app applies to its model.
#[derive(Debug, Clone)]
enum ToolEvent {
    /// Insert a MIDI note (pen tool in MIDI domain).
    Note { pitch: u8, velocity: u8, start: f64, duration: f64 },
    /// A model-level MIDI edit (index based, rebuild-safe).
    Midi(iced_liveplot::widget::MidiEdit),
    /// Delete a region (eraser, non-MIDI path).
    Delete { region: iced_liveplot::region::RegionId },
    /// Box-select the x/y range.
    BoxSelect { x0: f64, x1: f64, y0: f64, y1: f64 },
    /// A status message from the widget.
    Notify(String),
}

impl SequencerApp {
    fn new() -> (Self, Task<Message>) {
        let mut sequence = MidiSequence::new();
        sequence.tempo = 124.0;
        sequence.ticks_per_beat = PPQ;
        sequence.time_signature = (4, 4);
        sequence.loop_start = Some(0);
        sequence.loop_end = Some(PPQ * BARS as u64 * 4);

        // 16 tracks with distinct defaults
        for i in 0..TRACK_COUNT {
            let mut track = MidiTrack::new(format!("Track {}", i + 1));
            track.channel = (i % 16) as u8;
            track.program = (i * 8) as u8;
            track.volume = 100;
            track.pan = 64;
            sequence.add_track(track);
        }

        // Demo content: a chord progression on track 1 and 2, drums on 10/11
        Self::seed_demo(&mut sequence);

        let mut app = Self {
            sequence,
            active: 0,
            program: Program::new(Self::build_plot(0, Vec::new(), None)),
            roll_style: PianoRollStyle::default(),
            snap_engine: SnapEngine::new(SnapConfig::default()),
            selection: Vec::new(),
            tool: Tool::Grab,
            domain: Domain::Midi,
            note_length: NoteLength::Eighth,
            snap_enabled: true,
            snap_division: 4,
            tempo: 124.0,
            bars: BARS,
            show_names: true,
            time_axis: TimeAxisMode::Seconds,
            playhead: 0.0,
            playing: false,
            status: "16-track MIDI sequencer ready".to_string(),
            revision: 0,
            pending_tool_events: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            clipboard: Vec::new(),
        };

        app.apply_snap_config();
        app.rebuild_plot();
        app.reset_selection();
        (app, Task::none())
    }

    /// Build the plot for a track: piano-roll scatter series + note regions.
    fn build_plot(track_index: usize, regions: Vec<DataRegion>, keep: Option<iced_liveplot::Viewport>) -> Plot {
        let (r, g, b) = TRACK_COLORS[track_index];
        let series = Series::scatter(format!("Notes {}", track_index + 1)).with_kind(
            SeriesKind::Scatter(MarkerStyle {
                color: iced::Color {
                    r,
                    g,
                    b,
                    a: 1.0,
                },
                size: 4.0,
                shape: MarkerShape::Square,
            }),
        );

        let mut plot = Plot::builder()
            .id(format!("seq_track_{}", track_index))
            .theme(PlotTheme::dark())
            .x_axis(
                AxisConfig::builder()
                    .title(format!("Timeline — {} bars @ {} PPQ", BARS, PPQ))
                    .units("seconds")
                    .grid(true)
                    .minor_grid(true)
                    .build(),
            )
            .y_axis(
                AxisConfig::builder()
                    .title("Pitch (MIDI note numbers 0-127)")
                    .grid(true)
                    .zero_line(false)
                    .build(),
            )
            .build();
        plot.add_series(&series);
        for region in regions {
            plot.add_region(region);
        }
        // Piano roll Y axis: fixed 0..127 MIDI note range so clicks map to
        // pitches even when a track is empty (auto-fit would give 0..1).
        //  preserves the user's pan/zoom across model rebuilds —
        // rebuilding on every tool click must NOT snap the view back.
        let default_view = iced_liveplot::Viewport::new(
            iced_liveplot::view::Range::new(0.0, BARS as f64 * 4.0 * 60.0 / 124.0),
            iced_liveplot::view::Range::new(0.0, 127.0),
        );
        plot.set_manual_view(keep.unwrap_or(default_view));
        plot
    }

    /// Rebuild the widget program for the active track from the model.
    ///
    /// Regions are regenerated from `sequence.tracks[active].notes`, so every
    /// model mutation is reflected exactly once (single source of truth).
    /// NOTE: `Program::new` starts with no tool event handler — it must be
    /// re-registered on every rebuild, otherwise tools silently do nothing.
    fn rebuild_plot(&mut self) {
        let active = self.active;
        let style = &self.roll_style;
        // Preserve the current viewport so rebuilding after a tool click never
        // resets the user's pan/zoom.
        let keep = self.program.plot().viewport();
        let tempo = self.tempo;
        let mut regions: Vec<DataRegion> = self.sequence.tracks[active]
            .notes
            .iter()
            .enumerate()
            .map(|(index, note)| Self::note_region_at(note, style, index, tempo))
            .collect();

        // ---- SELECTION: re-apply by MODEL IDENTITY (pitch/ticks) ----
        // Region ids change on every rebuild, so the selection is re-derived
        // from the *note* each previously selected region pointed at.
        let wanted: Vec<MidiNote> = self
            .selection
            .iter()
            .filter_map(|id| self.selected_note_for_region(*id))
            .collect();
        self.selection.clear();
        for region in &mut regions {
            let RegionKind::MidiNote { pitch, velocity, start, end, channel } = &region.kind else {
                continue;
            };
            let hit = wanted.iter().any(|n| {
                n.pitch == *pitch
                    && n.velocity == *velocity
                    && n.channel == *channel
                    && n.start_tick == sec_to_ticks(*start, tempo)
                    && n.end_tick == sec_to_ticks(*end, tempo)
            });
            if hit {
                region.selected = true;
                self.selection.push(region.id);
            }
        }

        let plot = Self::build_plot(active, regions, keep);
        self.program = Program::new(plot);
        self.program.set_domain(self.domain);
        self.program.set_tool(self.tool);
        self.program.set_note_length(self.note_length);
        self.program.set_snap_engine(self.snap_engine.clone());
        let tpb = self.sequence.ticks_per_beat as u32;
        self.program.set_ticks_per_beat(tpb);
        self.program.set_tempo_bpm(self.tempo); // drives ticks↔seconds
        self.program
            .set_beats_per_bar(self.sequence.time_signature.0 as u32);
        self.program.set_time_axis(self.time_axis);
        // Keep the playhead position across the rebuild.
        let playhead = self.playhead_position();
        self.program.set_playhead_ticks(playhead);
        self.register_tool_handler();
        self.revision += 1;
    }

    /// Track-level selection: every note of the active track.
    fn select_all_track(&mut self) {
        self.selection = self.active_regions().iter().map(|r| r.id).collect();
        self.program.with_plot_mut(|plot| {
            for region in plot.regions_mut() {
                region.selected = true;
            }
        });
        let count = self.selection.len();
        self.status = format!("Selected all {} notes of track {}", count, self.active + 1);
        self.program.request_redraw();
    }

    /// NOTE: model index of a region id — the canonical mapping used by every
    /// tool handler. Region ids are ephemeral; `note_index` is stable.
    fn model_index_of(&self, region_id: iced_liveplot::region::RegionId) -> Option<usize> {
        self.program
            .plot()
            .regions()
            .iter()
            .find(|r| r.id == region_id)
            .and_then(|r| r.metadata.get("note_index").and_then(|v| v.as_u64()))
            .map(|i| i as usize)
    }

    // ========================================================================
    // Selection & track-level edit commands (buttons / keys)
    // ========================================================================

    /// Current selection as model indices (empty when nothing is selected).
    fn selection_indices(&self) -> Vec<usize> {
        let track_len = self.sequence.tracks[self.active].notes.len();
        self.target_indices(None, track_len)
    }

    /// Delete the current selection (track-level when nothing is selected).
    fn delete_selection(&mut self) {
        let track_len = self.sequence.tracks[self.active].notes.len();
        let mut indices = self.selection_indices();
        if indices.is_empty() {
            // No selection → the whole track (matches "Delete All").
            indices = (0..track_len).collect();
        }
        indices.sort_unstable_by(|a, b| b.cmp(a));
        let mut removed = 0;
        for index in indices {
            if index < self.sequence.tracks[self.active].notes.len() {
                self.sequence.tracks[self.active].notes.remove(index);
                removed += 1;
            }
        }
        self.reset_selection();
        self.status = format!("Deleted {} note(s)", removed);
        self.rebuild_plot();
    }

    /// Copy the selection into the model clipboard (cross-track).
    fn copy_selection(&mut self) {
        let indices = self.selection_indices();
        if indices.is_empty() {
            self.status = "Nothing to copy — select notes first".into();
            return;
        }
        let notes = &self.sequence.tracks[self.active].notes;
        self.clipboard = indices
            .iter()
            .filter_map(|i| notes.get(*i).copied())
            .collect();
        self.status = format!("Copied {} note(s)", self.clipboard.len());
    }

    /// Cut = copy + delete.
    fn cut_selection(&mut self) {
        self.copy_selection();
        self.delete_selection();
        self.status = format!("Cut {} note(s) to clipboard", self.clipboard.len());
        self.rebuild_plot();
    }

    /// Paste the clipboard at the playhead into the active track.
    fn paste_clipboard(&mut self) {
        if self.clipboard.is_empty() {
            self.status = "Clipboard empty".into();
            return;
        }
        let clip_start = self.clipboard.iter().map(|n| n.start_tick).min().unwrap_or(0);
        // Snap the target position when snapping is enabled: with snap OFF the
        // notes land at the EXACT playhead position; with snap ON they land on
        // the nearest grid line.
        // The playhead lives in TICKS in the widget; converting through
        // seconds with the CURRENT tempo keeps both representations identical
        // (a hard-coded tempo would shift the paste whenever the tempo changes).
        let playhead = if self.snap_enabled {
            let target_secs = self.program.playhead_seconds();
            let snapped = self.snap_engine.snap_position_seconds(target_secs);
            snapped
                .map(|s| sec_to_ticks(s, self.tempo))
                .unwrap_or_else(|| self.program.playhead_ticks())
        } else {
            // Snap off: the exact playhead tick (no rounding through seconds).
            self.program.playhead_ticks()
        };
        let offset = playhead.saturating_sub(clip_start);
        let shifted: Vec<MidiNote> = self
            .clipboard
            .iter()
            .map(|n| MidiNote::new(n.pitch, n.velocity, n.channel, n.start_tick + offset, n.end_tick + offset))
            .collect();
        self.sequence.paste_notes(self.active, &shifted, 0);
        self.status = format!(
            "Pasted {} note(s) into track {} at tick {} (playhead)",
            self.clipboard.len(),
            self.active + 1,
            playhead
        );
        self.rebuild_plot();
    }

    /// Quantize the selection (or the whole track when nothing is selected).
    fn quantize_track(&mut self) {
        self.apply_midi_edit(iced_liveplot::widget::MidiEdit::Quantize {
            targets: iced_liveplot::widget::NoteTargets { indices: Vec::new() },
        });
    }

    /// Transpose the selection (or the whole track when nothing is selected).
    fn transpose_track(&mut self, semitones: i8) {
        self.apply_midi_edit(iced_liveplot::widget::MidiEdit::Transpose {
            targets: iced_liveplot::widget::NoteTargets { indices: Vec::new() },
            semitones,
        });
    }

    /// Set the velocity of the selection (or the whole track).
    fn set_selected_velocity(&mut self, velocity: i64) {
        self.apply_midi_edit(iced_liveplot::widget::MidiEdit::Velocity {
            targets: iced_liveplot::widget::NoteTargets { indices: Vec::new() },
            velocity: velocity.clamp(0, 127) as u8,
        });
    }

    /// Select all notes of the active track.
    fn select_all_regions(&mut self) {
        self.select_all_track();
    }

    /// Clear the current selection.
    fn clear_selection(&mut self) {
        self.reset_selection();
        self.program.request_redraw();
        self.status = "Selection cleared".into();
    }

    // ========================================================================
    // Model configuration
    // ========================================================================

    /// Apply the current snap configuration to the engine (and mirror into
    /// the widget program).
    fn apply_snap_config(&mut self) {
        let (num, den) = self.sequence.time_signature;
        // One beat (in the denominator's unit) at the current tempo.
        let beat_unit = 60.0 / self.tempo * (4.0 / den as f64);
        // division per bar: 4 = quarter, 8 = eighth, 16 = 16th, 32 = 32nd
        let sixteenth = beat_unit / 4.0;
        let step = sixteenth * (16 / self.snap_division.max(1)) as f64;
        *self.snap_engine.config_mut() = SnapConfig {
            enabled: self.snap_enabled,
            grid: Some(GridSpec::seconds(step.max(1e-4))),
            time_base: TimeBase::Beats,
            threshold_px: 12.0,
            snap_to_regions: false,
            snap_region_kinds: vec![],
            gravity: 0.5,
        };
        let engine = self.snap_engine.clone();
        self.program.set_snap_engine(engine);
    }

    fn reset_selection(&mut self) {
        self.selection.clear();
        self.program.with_plot_mut(|plot| {
            for region in plot.regions_mut() {
                region.selected = false;
            }
        });
    }

    /// The playhead in seconds.
    fn playhead_seconds(&self) -> f64 {
        self.program.playhead_seconds()
    }

    /// Ticks → seconds at the current tempo.
    fn ticks_to_sec(&self, ticks: u64) -> f64 {
        MidiNote::tick_to_seconds_static(ticks, PPQ, self.tempo)
    }

    fn note_length_seconds(&self) -> f64 {
        self.note_length.seconds(self.tempo)
    }

    /// Seed a demo pattern (chords + bass + drums).
    fn seed_demo(sequence: &mut MidiSequence) {
        let chord_roots: [(u8, i8); 4] = [(0, 0), (5, 2), (7, 4), (5, -3)]; // C, F, G, Am
        // Track 1: chords (triads), 8 bars
        {
            let track = &mut sequence.tracks[0];
            for bar in 0..(BARS as u64 / 2) {
                for (i, (root, third)) in chord_roots.iter().enumerate() {
                    let bar_start = bar * 2 * PPQ * 4;
                    let start = bar_start + i as u64 * PPQ * 2;
                    let end = start + PPQ * 2;
                    let pitches = [
                        60 + *root as i16 + *third as i16,
                        64 + *root as i16 + *third as i16,
                        67 + *root as i16 + *third as i16,
                    ];
                    for p in pitches {
                        track.add_note(MidiNote::new(p.clamp(0, 127) as u8, 90, 0, start, end));
                    }
                }
            }
        }
        // Track 2: bass line
        {
            let track = &mut sequence.tracks[1];
            for bar in 0..(BARS as u64 / 4) {
                for beat in 0..4u64 {
                    let start = (bar * 4 + beat) * PPQ;
                    track.add_note(MidiNote::new(
                        36 + (beat as u8 % 4) * 5,
                        100,
                        1,
                        start,
                        start + PPQ / 2,
                    ));
                }
            }
        }
        // Track 9 (index 9): kick
        {
            let track = &mut sequence.tracks[9];
            for bar in 0..(BARS as u64) {
                for beat in 0..4u64 {
                    let start = (bar * 4 + beat) * PPQ;
                    track.add_note(MidiNote::new(36, 110, 9, start, start + PPQ / 8));
                }
            }
        }
        // Track 10 (index 10): hats on offbeats
        {
            let track = &mut sequence.tracks[10];
            for bar in 0..(BARS as u64) {
                for eighth in 0..8u64 {
                    if eighth % 2 == 1 {
                        let start = bar * 4 * PPQ + eighth * PPQ / 2;
                        track.add_note(MidiNote::new(42, 70, 10, start, start + PPQ / 8));
                    }
                }
            }
        }
    }

    /// The playhead position mirrored into the widget.
    fn playhead_position(&self) -> u64 {
        self.program.playhead_ticks()
    }

    // ========================================================================
    // Tool event pipeline (single entry point for every tool edit)
    // ========================================================================

    /// Regions of the active track (the current mirror of the model).
    fn active_regions(&self) -> Vec<DataRegion> {
        self.program.plot().regions().to_vec()
    }

    /// Convert a `MidiNote` into a render region, colored by velocity.
    ///
    /// `index` is the note's position in the active track's `notes` vector —
    /// written into `region.metadata["note_index"]` so tools address notes by
    /// MODEL INDEX (stable across plot rebuilds) instead of ephemeral
    /// region ids.
    fn note_region_at(
        note: &MidiNote,
        style: &PianoRollStyle,
        index: usize,
        tempo: f64,
    ) -> DataRegion {
        let start = MidiNote::tick_to_seconds_static(note.start_tick, PPQ, tempo);
        let end = MidiNote::tick_to_seconds_static(note.end_tick, PPQ, tempo);
        let mut region = DataRegion::midi_note(note.pitch, note.velocity, note.channel, start, end);
        // Color by velocity using the piano roll gradient.
        let t = (note.velocity as f32 / 127.0) * (style.velocity_gradient.len() - 1) as f32;
        let idx = (t.round() as usize).min(style.velocity_gradient.len() - 1);
        region.style.fill = Some(style.velocity_gradient[idx]);
        region.style.stroke = Some(style.note_stroke);
        region
            .metadata
            .insert("note_index".to_string(), serde_json::json!(index));
        region
    }

    /// Register the widget→app tool event pipe. `Program::new` starts WITHOUT
    /// a handler, so this must run on every rebuild.
    fn register_tool_handler(&self) {
        let pending = self.pending_tool_events.clone();
        self.program
            .set_tool_event_handler(std::rc::Rc::new(move |event| {
                let mut buf = pending.borrow_mut();
                use iced_liveplot::widget::{MidiEdit, ResizeEdge, ToolEvent as TE};
                match event {
                    TE::Midi(edit) => {
                        buf.push(ToolEvent::Midi(edit));
                    }
                    TE::InsertMidiNote { pitch, velocity, start, duration } => {
                        buf.push(ToolEvent::Note {
                            pitch,
                            velocity,
                            start,
                            duration,
                        });
                    }
                    TE::DeleteRegion { region } => {
                        buf.push(ToolEvent::Delete { region });
                    }
                    TE::BoxSelect { x0, x1, y0, y1 } => {
                        buf.push(ToolEvent::BoxSelect { x0, x1, y0, y1 });
                    }
                    TE::Notify(msg) => buf.push(ToolEvent::Notify(msg)),
                    _ => {}
                }
                true
            }));
    }

    /// Apply the tool events queued by the widget since the last frame.
    ///
    /// Every branch goes through `self.target_indices()` which implements the
    /// two-level edit model:
    /// * **selection present** → edit the whole selection;
    /// * **no selection** → edit the single event under the cursor;
    /// * track-level actions (buttons/keys) pass `None` → whole track.
    fn drain_tool_events(&mut self) {
        let events: Vec<ToolEvent> = std::mem::take(&mut *self.pending_tool_events.borrow_mut());
        for event in events {
            match event {
                ToolEvent::Note { pitch, velocity, start, duration } => {
                    let start_tick = sec_to_ticks(start, self.tempo);
                    let end_tick = sec_to_ticks(start + duration, self.tempo);
                    let channel = self.sequence.tracks[self.active].channel;
                    self.sequence.tracks[self.active].add_note(MidiNote::new(
                        pitch,
                        velocity,
                        channel,
                        start_tick,
                        end_tick.max(start_tick + 1),
                    ));
                    self.status = format!(
                        "Pen: {} (vel {}) at {:.2}s, {}",
                        midi_note_name(pitch),
                        velocity,
                        start,
                        self.note_length.label()
                    );
                    self.rebuild_plot();
                }
                ToolEvent::Midi(edit) => self.apply_midi_edit(edit),
                ToolEvent::Delete { region } => {
                    if let Some(index) = self.model_index_of(region) {
                        let notes = &mut self.sequence.tracks[self.active].notes;
                        if index < notes.len() {
                            notes.remove(index);
                        }
                        self.status = format!("Erased note #{}", index);
                    }
                    self.rebuild_plot();
                }
                ToolEvent::BoxSelect { x0, x1, y0, y1 } => self.tool_box_select(x0, x1, y0, y1),
                ToolEvent::Notify(msg) => self.status = msg,
            }
        }
    }

    /// Resolve which model indices an edit applies to.
    ///
    /// `hit` is the index under the cursor (or `None` for track-level actions).
    /// When a box-selection exists it always wins, so tools operate on the
    /// selection exactly like commercial DAWs.
    fn target_indices(&self, hit: Option<usize>, track_len: usize) -> Vec<usize> {
        if self.selection.is_empty() {
            return hit.map(|i| vec![i]).unwrap_or_default();
        }
        // Selection wins when there is one, otherwise fall back to the hit.
        let mut out: Vec<usize> = Vec::new();
        for id in &self.selection {
            if let Some(index) = self.model_index_of(*id) {
                if index < track_len && !out.contains(&index) {
                    out.push(index);
                }
            }
        }
        if out.is_empty() {
            hit.map(|i| vec![i]).unwrap_or_default()
        } else {
            out
        }
    }

    /// Apply one model-level MIDI edit coming from a tool or a button.
    fn apply_midi_edit(&mut self, edit: iced_liveplot::widget::MidiEdit) {
        use iced_liveplot::widget::{MidiEdit, ResizeEdge};
        let track_len = self.sequence.tracks[self.active].notes.len();
        match edit {
            MidiEdit::Move { targets, dt_ticks, dy_semitones } => {
                let indices = self.target_indices(targets.indices.first().copied(), track_len);
                let mut moved = 0;
                let notes = &mut self.sequence.tracks[self.active].notes;
                for index in indices {
                    if index < track_len {
                        let note = &mut notes[index];
                        let start = note.start_tick as i64 + dt_ticks;
                        let end = note.end_tick as i64 + dt_ticks;
                        note.start_tick = start.max(0) as u64;
                        note.end_tick = end.max(note.start_tick as i64 + 1) as u64;
                        note.pitch = (note.pitch as i16 + dy_semitones as i16).clamp(0, 127) as u8;
                        moved += 1;
                    }
                }
                self.status = format!("Moved {} note(s) ({:+} ticks, {:+} st)", moved, dt_ticks, dy_semitones);
            }
            MidiEdit::Resize { targets, edge, dt_ticks } => {
                // Trim semantics (Logic Pro): the same cursor travel is applied
                // to every target, so grouped events keep their relative
                // lengths. Each event is independently clamped so it never
                // collapses (min 1 tick) and never starts before 0.
                let indices = self.target_indices(targets.indices.first().copied(), track_len);
                let mut resized = 0;
                let notes = &mut self.sequence.tracks[self.active].notes;
                for index in indices {
                    if index < track_len {
                        let note = &mut notes[index];
                        match edge {
                            ResizeEdge::Start => {
                                let want = note.start_tick as i64 + dt_ticks;
                                // keep at least 1 tick of the note, and >= 0
                                let limit = (note.end_tick as i64 - 1).max(0);
                                note.start_tick = want.clamp(0, limit) as u64;
                            }
                            ResizeEdge::End => {
                                let want = note.end_tick as i64 + dt_ticks;
                                let floor = note.start_tick as i64 + 1;
                                note.end_tick = want.max(floor) as u64;
                            }
                        }
                        resized += 1;
                    }
                }
                self.status = format!(
                    "Trimmed {} edge of {} note(s) by {:+} ticks",
                    match edge {
                        ResizeEdge::Start => "start",
                        ResizeEdge::End => "end",
                    },
                    resized,
                    dt_ticks
                );
            }
            MidiEdit::Delete { targets } => {
                let mut indices = self.target_indices(targets.indices.first().copied(), track_len);
                indices.sort_unstable_by(|a, b| b.cmp(a));
                let mut removed = 0;
                let notes = &mut self.sequence.tracks[self.active].notes;
                for index in indices {
                    if index < notes.len() {
                        notes.remove(index);
                        removed += 1;
                    }
                }
                self.selection.clear();
                self.status = format!("Deleted {} note(s)", removed);
            }
            MidiEdit::Split { targets, tick } => {
                let indices = self.target_indices(targets.indices.first().copied(), track_len);
                let mut splits = 0;
                // Insert in descending order so earlier indices stay valid.
                let mut indices = indices;
                indices.sort_unstable_by(|a, b| b.cmp(a));
                let notes = &mut self.sequence.tracks[self.active].notes;
                for index in indices {
                    if index < notes.len() {
                        let note = notes[index];
                        if tick > note.start_tick && tick < note.end_tick {
                            let left = MidiNote::new(note.pitch, note.velocity, note.channel, note.start_tick, tick);
                            let right = MidiNote::new(note.pitch, note.velocity, note.channel, tick, note.end_tick);
                            notes[index] = left;
                            notes.insert(index + 1, right);
                            splits += 1;
                        }
                    }
                }
                self.status = format!("Split {} note(s) at tick {}", splits, tick);
            }
            MidiEdit::SelectAll => self.select_all_track(),
            MidiEdit::ClearSelection => self.reset_selection(),
            MidiEdit::Quantize { targets } => {
                let indices = self.target_indices(targets.indices.first().copied(), track_len);
                let indices = if indices.is_empty() {
                    (0..track_len).collect::<Vec<_>>()
                } else {
                    indices
                };
                let notes = &mut self.sequence.tracks[self.active].notes;
                let step = match self.note_length {
                    NoteLength::Sixteenth => PPQ as u64 / 4,
                    NoteLength::Eighth => PPQ as u64 / 2,
                    NoteLength::Quarter => PPQ as u64,
                    NoteLength::Half => PPQ as u64 * 2,
                    NoteLength::Whole => PPQ as u64 * 4,
                };
                for index in indices {
                    if index < track_len {
                        let note = &mut notes[index];
                        note.start_tick = (note.start_tick / step.max(1)) * step.max(1);
                        note.end_tick = note.end_tick.max(note.start_tick + 1);
                    }
                }
                self.status = format!("Quantized to {} grid", self.note_length.label());
            }
            MidiEdit::Transpose { targets, semitones } => {
                let indices = self.target_indices(targets.indices.first().copied(), track_len);
                let indices = if indices.is_empty() {
                    (0..track_len).collect::<Vec<_>>()
                } else {
                    indices
                };
                let notes = &mut self.sequence.tracks[self.active].notes;
                for index in indices {
                    if index < track_len {
                        let note = &mut notes[index];
                        note.pitch = (note.pitch as i16 + semitones as i16).clamp(0, 127) as u8;
                    }
                }
                self.status = format!("Transposed by {:+} semitones", semitones);
            }
            MidiEdit::Velocity { targets, velocity } => {
                let indices = self.target_indices(targets.indices.first().copied(), track_len);
                let indices = if indices.is_empty() {
                    (0..track_len).collect::<Vec<_>>()
                } else {
                    indices
                };
                let notes = &mut self.sequence.tracks[self.active].notes;
                for index in indices {
                    if index < track_len {
                        notes[index].velocity = velocity;
                    }
                }
                self.status = format!("Velocity set to {}", velocity);
            }
            MidiEdit::Insert { pitch, velocity, start_tick, end_tick } => {
                let notes = &mut self.sequence.tracks[self.active].notes;
                notes.push(MidiNote::new(pitch, velocity, 0, start_tick, end_tick));
                self.status = format!("Inserted {}", midi_note_name(pitch));
            }
        }
        self.rebuild_plot();
    }

    /// Resolve a selected region id back to its note (for rebuild restore).
    fn selected_note_for_region(&self, region_id: iced_liveplot::region::RegionId) -> Option<MidiNote> {
        let index = self.note_index_for_region(region_id)?;
        self.sequence.tracks.get(self.active)?.notes.get(index).copied()
    }

    /// Series id of the active track's plot series.
    fn series_for_active(&self) -> Option<iced_liveplot::series::SeriesId> {
        self.program.plot().series().first().map(|s| s.id())
    }

    /// Map a plot region id back to a note index (both are in note order).
    fn note_index_for_region(&self, region_id: iced_liveplot::region::RegionId) -> Option<usize> {
        let regions = self.active_regions();
        regions.iter().position(|r| r.id == region_id)
    }

    fn tool_move_region(&mut self, region: iced_liveplot::region::RegionId, dt: f64, dy: i8) {
        // The widget emits one event per target (the grabbed note plus every
        // box-selected region), so each application moves exactly one note.
        let Some(index) = self.note_index_for_region(region) else {
            self.status = "Move: note not found".into();
            return;
        };
        if index >= self.sequence.tracks[self.active].notes.len() {
            return;
        }
        // Read-modify in small steps so we never hold a borrow across helpers.
        let tempo = self.tempo;
        let to_ticks = |s: f64| -> u64 {
            let beats = s * tempo / 60.0;
            (beats * PPQ as f64).round() as u64
        };
        let to_secs = |t: u64| -> f64 { MidiNote::tick_to_seconds_static(t, PPQ, tempo) };
        let note = &mut self.sequence.tracks[self.active].notes[index];
        let start = to_ticks(to_secs(note.start_tick) + dt).max(0);
        let end = to_ticks(to_secs(note.end_tick) + dt).max(start + 1);
        note.start_tick = start;
        note.end_tick = end;
        note.pitch = (note.pitch as i16 + dy as i16).clamp(0, 127) as u8;
        let pitch = note.pitch;
        self.sequence.tracks[self.active].notes.sort_by_key(|n| n.start_tick);
        self.status = format!("Moved {} by {:.2}s / {:+} st", midi_note_name(pitch), dt, dy);
        self.rebuild_plot();
    }

    fn tool_resize_region(
        &mut self,
        region: iced_liveplot::region::RegionId,
        edge: iced_liveplot::widget::ResizeEdge,
        time: f64,
    ) {
        let Some(index) = self.note_index_for_region(region) else {
            self.status = "Resize: note not found".into();
            return;
        };
        let notes = &mut self.sequence.tracks[self.active].notes;
        if index >= notes.len() {
            return;
        }
        let tempo = self.tempo;
        let tick = {
            let beats = time * tempo / 60.0;
            (beats * PPQ as f64).round() as u64
        };
        match edge {
            iced_liveplot::widget::ResizeEdge::Start => {
                notes[index].start_tick = tick.min(notes[index].end_tick.saturating_sub(1));
            }
            iced_liveplot::widget::ResizeEdge::End => {
                notes[index].end_tick = tick.max(notes[index].start_tick + 1);
            }
        }
        self.status = format!(
            "Resized {} edge to {:.2}s ({} ticks)",
            match edge {
                iced_liveplot::widget::ResizeEdge::Start => "start",
                iced_liveplot::widget::ResizeEdge::End => "end",
            },
            time,
            tick
        );
        self.rebuild_plot();
    }

    /// Split a note at `time` into two notes. Returns true when the note was
    /// actually split (the time falls strictly inside it).
    fn tool_split_region(
        &mut self,
        region: iced_liveplot::region::RegionId,
        time: f64,
    ) -> bool {
        let Some(index) = self.note_index_for_region(region) else {
            return false;
        };
        let tempo = self.tempo;
        let tick = sec_to_ticks(time, tempo);
        let notes = &mut self.sequence.tracks[self.active].notes;
        if index >= notes.len() {
            return false;
        }
        let note = notes[index];
        if tick <= note.start_tick || tick >= note.end_tick {
            return false;
        }
        // Left half keeps the original, right half is appended right after.
        let left = note;
        let right = MidiNote::new(
            note.pitch,
            note.velocity,
            note.channel,
            tick,
            note.end_tick,
        );
        let right_id = right;
        notes[index] = MidiNote::new(left.pitch, left.velocity, left.channel, left.start_tick, tick);
        notes.insert(index + 1, right);
        let _ = right_id;
        notes.sort_by_key(|n| n.start_tick);
        true
    }

    fn tool_delete_region(&mut self, region: iced_liveplot::region::RegionId) {
        let Some(index) = self.note_index_for_region(region) else {
            self.status = "Erase: no note under cursor".into();
            return;
        };
        let notes = &mut self.sequence.tracks[self.active].notes;
        if index < notes.len() {
            let note = notes.remove(index);
            self.status = format!("Erased {}", midi_note_name(note.pitch));
        }
        self.rebuild_plot();
    }

    fn tool_box_select(&mut self, x0: f64, x1: f64, y0: f64, y1: f64) {
        let (x_min, x_max) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
        let (y_min, y_max) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
        let selected: Vec<_> = self
            .active_regions()
            .into_iter()
            .filter(|r| {
                r.time_range()
                    .is_some_and(|t| t.max >= x_min && t.min <= x_max)
                    && r.value_range()
                        .is_some_and(|v| v.max >= y_min && v.min <= y_max)
            })
            .map(|r| r.id)
            .collect();
        let count = selected.len();
        self.selection = selected;
        // Mirror the selection into the plot regions so the yellow handles and
        // the highlight appear, and so group edits (move/resize) see them.
        self.program.with_plot_mut(|plot| {
            for region in plot.regions_mut() {
                region.selected = self.selection.contains(&region.id);
            }
        });
        self.status = format!("Box select: {} notes in x[{:.1}, {:.1}]", count, x_min, x_max);
        self.program.request_redraw();
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        // Drain tool events emitted by the widget since the last update
        self.drain_tool_events();
        match message {
            Message::SelectTrack(index) => {
                self.active = index.min(TRACK_COUNT - 1);
                self.reset_selection();
                self.rebuild_plot();
                self.status = format!(
                    "Track {}: {} notes{}",
                    self.active + 1,
                    self.sequence.tracks[self.active].notes.len(),
                    if self.sequence.tracks[self.active].mute { " (muted)" } else { "" }
                );
            }
            Message::ToggleMute(index) => {
                self.sequence.tracks[index].mute = !self.sequence.tracks[index].mute;
                self.status = format!(
                    "Track {} {}",
                    index + 1,
                    if self.sequence.tracks[index].mute { "muted" } else { "unmuted" }
                );
            }
            Message::ToggleSolo(index) => {
                self.sequence.tracks[index].solo = !self.sequence.tracks[index].solo;
                self.status = format!(
                    "Track {} {}",
                    index + 1,
                    if self.sequence.tracks[index].solo { "solo" } else { "un-solo" }
                );
            }
            Message::CycleTrackName(index) => {
                const NAMES: [&str; 8] = [
                    "Drums", "Bass", "Chords", "Lead", "Pads", "Arp", "FX", "Vox",
                ];
                let n = &mut self.sequence.tracks[index].name;
                let current = NAMES.iter().position(|x| x == n).unwrap_or(0);
                *n = NAMES[(current + 1) % NAMES.len()].to_string();
                self.status = format!("Track {} renamed to {}", index + 1, n);
            }
            Message::SetProgram(index, value) => {
                self.sequence.tracks[index].program = value.clamp(0, 127) as u8;
                self.status = format!("Track {} program = {}", index + 1, value);
            }
            Message::SetVolume(index, value) => {
                self.sequence.tracks[index].volume = value.clamp(0, 127) as u8;
                self.status = format!("Track {} volume = {}", index + 1, value);
            }
            Message::SetPan(index, value) => {
                self.sequence.tracks[index].pan = value.clamp(0, 127) as u8;
                self.status = format!("Track {} pan = {}", index + 1, value);
            }
            Message::SetTool(tool) => {
                self.tool = tool;
                self.program.set_tool(tool);
                self.status = format!("Tool: {} — {}", tool.name(), tool.description());
            }
            Message::SetNoteLength(len) => {
                self.note_length = len;
                self.program.set_note_length(len);
                self.status = format!("Note length: {} ({} ticks)", len.label(), len.ticks(PPQ as u32));
            }
            Message::SetDomain(domain) => {
                self.domain = domain;
                self.program.set_domain(domain);
                self.status = format!("Domain: {} (Y axis + tool behavior)", domain.name());
            }
            Message::SetSnapDivision(division) => {
                self.snap_division = division.clamp(1, 32);
                self.apply_snap_config();
                self.status = format!("Snap division: 1/{}", self.snap_division);
            }
            Message::ToggleSnap => {
                self.snap_enabled = !self.snap_enabled;
                self.apply_snap_config();
                self.status = format!("Snap: {}", if self.snap_enabled { "ON" } else { "OFF" });
            }
            Message::Undo => {
                if self.program.undo() {
                    self.status = "Undo".into();
                    self.rebuild_plot();
                } else {
                    self.status = "Undo: nothing to undo".into();
                }
            }
            Message::Redo => {
                if self.program.redo() {
                    self.status = "Redo".into();
                    self.rebuild_plot();
                } else {
                    self.status = "Redo: nothing to redo".into();
                }
            }
            Message::Copy => self.copy_selection(),
            Message::Cut => self.cut_selection(),
            Message::Paste => self.paste_clipboard(),
            Message::DeleteSelection => {
                self.delete_selection();
                self.rebuild_plot();
            }
            Message::ClearSelection => {
                self.reset_selection();
                self.program.request_redraw();
                self.status = "Selection cleared".into();
            }
            Message::SelectAllTrack => self.select_all_track(),
            Message::QuantizeTrack => self.quantize_track(),
            Message::TransposeTrack(semi) => self.transpose_track(semi),
            Message::SetVelocity(v) => self.set_selected_velocity(v),
            Message::SetTempo(bpm) => {
                self.tempo = bpm.clamp(40.0, 300.0);
                self.sequence.tempo = self.tempo;
                self.program.set_tempo_bpm(self.tempo);
                self.apply_snap_config();
                self.rebuild_plot();
                self.status = format!("Tempo: {:.0} BPM", self.tempo);
            }
            Message::ToggleLoop => {
                let total = self.sequence.total_duration_ticks().max(PPQ * BARS as u64 * 4);
                if self.sequence.loop_start.is_some() {
                    self.sequence.loop_start = None;
                    self.sequence.loop_end = None;
                    self.status = "Loop: OFF".into();
                } else {
                    self.sequence.loop_start = Some(0);
                    self.sequence.loop_end = Some(total);
                    self.status = format!("Loop: 0 — {:.1}s", self.ticks_to_sec(total));
                }
            }
            Message::ZoomBars(delta) => {
                self.bars = (self.bars as i64 + delta as i64).clamp(1, 64) as u32;
                self.rebuild_plot();
                self.status = format!("Zoom: {} bars visible", self.bars);
            }
            Message::ToggleTimeAxis => {
                self.time_axis = self.time_axis.next();
                self.program.set_time_axis(self.time_axis);
                self.status = format!("X axis: {}", self.time_axis.label());
            }
            Message::CycleTimeSignature => {
                const SIGS: [(u8, u8); 8] = [
                    (4, 4),
                    (3, 4),
                    (5, 4),
                    (7, 4),
                    (6, 8),
                    (7, 8),
                    (9, 8),
                    (13, 8),
                ];
                let (num, den) = self.sequence.time_signature;
                let idx = SIGS
                    .iter()
                    .position(|(n, d)| *n == num && *d == den)
                    .unwrap_or(0);
                self.sequence.time_signature = SIGS[(idx + 1) % SIGS.len()];
                let (num, den) = self.sequence.time_signature;
                self.status = format!("Time signature: {}/{} (bar = {} ticks)", num, den, num as u64 * PPQ * 4 / den as u64);
                // The numerator drives the bar.length used by the bar.beat axis.
                self.program.set_beats_per_bar(num as u32);
                self.apply_snap_config();
            }
            Message::ToggleNoteNames => {
                self.show_names = !self.show_names;
                self.roll_style.show_note_names = self.show_names;
                self.status = format!("Note names: {}", if self.show_names { "ON" } else { "OFF" });
            }
            Message::KeyPressed(key, modifiers) => {
                self.on_key(key, modifiers);
            }
            Message::Tick => {
                // Advance the transport clock (native monotonic time), then
                // mirror its position into the widget playhead for rendering.
                let delta = self.program.tick_transport();
                if delta > 0 {
                    self.playing = self.program.is_playing();
                    self.playhead = self.program.playhead_seconds();
                }
            }
            Message::TogglePlay => {
                if self.program.is_playing() {
                    self.program.pause();
                    self.playing = false;
                    self.status = "Stop".into();
                } else {
                    self.program.set_transport_bpm(self.tempo);
                    self.program.play();
                    self.playing = true;
                    self.status = "Play".into();
                }
            }
            Message::SeekStart => {
                self.program.set_playhead_ticks(0);
                self.playhead = 0.0;
                self.status = "Playhead at start".into();
            }
            Message::Plot(plot_message) => {
                // Hover / pin / reset handled by the widget itself.
                let _ = plot_message;
            }
        }
        Task::none()
    }

    fn on_key(&mut self, key: Key, modifiers: Modifiers) -> Task<Message> {
        use iced_liveplot::widget::StandardAction;
        if let (Key::Character(c), Modifiers::NONE) = (&key, modifiers) {
            if c.as_str() == " " {
                return self.update(Message::TogglePlay);
            }
            if c.as_str() == "0" {
                return self.update(Message::SeekStart);
            }
        }
        let Some(action) = self.key_action(&key, modifiers) else {
            return Task::none();
        };
        match action {
            StandardAction::Undo => self.update(Message::Undo),
            StandardAction::Redo => self.update(Message::Redo),
            StandardAction::Copy => self.update(Message::Copy),
            StandardAction::Cut => self.update(Message::Cut),
            StandardAction::Paste => self.update(Message::Paste),
            StandardAction::Delete => self.update(Message::DeleteSelection),
            StandardAction::SelectAll => self.update(Message::SelectAllTrack),
            StandardAction::DeselectAll => self.update(Message::ClearSelection),
            StandardAction::ToggleSnap => self.update(Message::ToggleSnap),
            StandardAction::ToolGrab => self.update(Message::SetTool(Tool::Grab)),
            StandardAction::ToolSelect => self.update(Message::SetTool(Tool::Select)),
            StandardAction::ToolPen => self.update(Message::SetTool(Tool::Pen)),
            StandardAction::ToolEraser => self.update(Message::SetTool(Tool::Eraser)),
            StandardAction::ToolMove => self.update(Message::SetTool(Tool::Move)),
            StandardAction::ToolScale => self.update(Message::SetTool(Tool::Scale)),
            StandardAction::ToolStretch => self.update(Message::SetTool(Tool::Stretch)),
            StandardAction::ToolSplit => self.update(Message::SetTool(Tool::Split)),
            _ => Task::none(),
        }
    }

    fn key_action(&self, key: &Key, modifiers: Modifiers) -> Option<iced_liveplot::widget::StandardAction> {
        use iced_liveplot::widget::StandardAction;
        match (&key, modifiers) {
            (Key::Character(c), Modifiers::CTRL) if c == "z" => Some(StandardAction::Undo),
            (Key::Character(c), Modifiers::CTRL) if c == "y" => Some(StandardAction::Redo),
            (Key::Character(c), Modifiers::CTRL) if c == "c" => Some(StandardAction::Copy),
            (Key::Character(c), Modifiers::CTRL) if c == "x" => Some(StandardAction::Cut),
            (Key::Character(c), Modifiers::CTRL) if c == "v" => Some(StandardAction::Paste),
            (Key::Character(c), Modifiers::CTRL) if c == "a" => Some(StandardAction::SelectAll),
            (Key::Character(c), Modifiers::CTRL) if c == "s" => Some(StandardAction::ToggleSnap),
            (_, _) if false => Some(StandardAction::Delete),
            (_, _) if false => Some(StandardAction::DeselectAll),
            (Key::Character(c), Modifiers::NONE) => match c.as_str() {
                "g" => Some(StandardAction::ToolGrab),
                "v" => Some(StandardAction::ToolSelect),
                "b" => Some(StandardAction::ToolPen),
                "e" => Some(StandardAction::ToolEraser),
                "m" => Some(StandardAction::ToolMove),
                "r" => Some(StandardAction::ToolScale),
                "t" => Some(StandardAction::ToolStretch),
                "p" => Some(StandardAction::ToolSplit),
                _ => None,
            },
            _ => None,
        }
    }
}

// ============================================================================
// VIEW
// ============================================================================

impl SequencerApp {
    fn view(&self) -> impl Widget<Message> + use<'_> {
        let sidebar = self.build_track_list();
        let editor = self.build_editor();
        let toolbar = self.build_toolbar();
        let status = container(
            row![
                text(self.status.clone()).size(12),
                Space::new(),
                text(format!(
                    "{} notes | undo={} redo={} | playhead {} ticks ({}s{})",
                    self.sequence.tracks[self.active].notes.len(),
                    self.program.can_undo(),
                    self.program.can_redo(),
                    self.program.playhead_ticks(),
                    self.program.playhead_seconds(),
                    if self.playing { " [PLAY]" } else { "" },
                ))
                .size(12),
            ]
            .spacing(8),
        )
        .padding([6, 12])
        .style(|theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(IcedColor::from_rgb(0.10, 0.10, 0.13))),
            ..Default::default()
        });

        let body = row![sidebar, editor].spacing(0).height(Length::Fill);
        let all = column![toolbar, body, status].spacing(0);
        container(all)
            .width(Length::Fill)
            .height(Length::Fill)
    }

    fn build_toolbar(&self) -> impl Widget<Message> + use<'_> {
        let tools = row![
            self.tool_button(Tool::Grab, "Grab (G)"),
            self.tool_button(Tool::Select, "Select (V)"),
            self.tool_button(Tool::Pen, "Pen (B)"),
            self.tool_button(Tool::Eraser, "Erase (E)"),
            self.tool_button(Tool::Move, "Move (M)"),
            self.tool_button(Tool::Scale, "Start edge (R)"),
            self.tool_button(Tool::Stretch, "End edge (T)"),
            self.tool_button(Tool::Split, "Split (P)"),
        ]
        .spacing(4);

        let lengths = row![
            button(text(self.length_label(NoteLength::Sixteenth)).size(11))
                .on_press(Message::SetNoteLength(NoteLength::Sixteenth)),
            button(text(self.length_label(NoteLength::Eighth)).size(11))
                .on_press(Message::SetNoteLength(NoteLength::Eighth)),
            button(text(self.length_label(NoteLength::Quarter)).size(11))
                .on_press(Message::SetNoteLength(NoteLength::Quarter)),
            button(text(self.length_label(NoteLength::Half)).size(11))
                .on_press(Message::SetNoteLength(NoteLength::Half)),
            button(text(self.length_label(NoteLength::Whole)).size(11))
                .on_press(Message::SetNoteLength(NoteLength::Whole)),
        ]
        .spacing(4);

        let history = row![
            button("Undo").on_press(Message::Undo),
            button("Redo").on_press(Message::Redo),
            button("Copy").on_press(Message::Copy),
            button("Cut").on_press(Message::Cut),
            button("Paste").on_press(Message::Paste),
            button("Delete").on_press(Message::DeleteSelection),
        ]
        .spacing(4);

        let snapshot = row![
            button("All").on_press(Message::SelectAllTrack),
            button("Quantize").on_press(Message::QuantizeTrack),
            button("-12st").on_press(Message::TransposeTrack(-12)),
            button("-1st").on_press(Message::TransposeTrack(-1)),
            button("+1st").on_press(Message::TransposeTrack(1)),
            button("+12st").on_press(Message::TransposeTrack(12)),
        ]
        .spacing(4);

        let transport = row![
            button(text(if self.playing { "⏸ Stop" } else { "▶ Play" }).size(11))
                .on_press(Message::TogglePlay),
            button("⏮").on_press(Message::SeekStart),
            text(format!(
                "{:.1}s · {}",
                self.program.playhead_seconds(),
                self.program
                    .clock()
                    .bar_beat(4)
                    .0
            ))
            .size(11),
        ]
        .spacing(4);

        let axes = row![
            button(text(format!("{} bars", self.bars)).size(11)).on_press(Message::ZoomBars(0)),
            button("-").on_press(Message::ZoomBars(-2)),
            button("+").on_press(Message::ZoomBars(2)),
            button(text(if self.show_names { "Names ON" } else { "Names OFF" }).size(11))
                .on_press(Message::ToggleNoteNames),
        ]
        .spacing(4);

        container(
            row![
                text("16-Track MIDI Sequencer").size(20),
                tools,
                lengths,
                history,
                transport,
                snapshot,
                axes,
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding(8)
        .width(Length::Fill)
        .style(|_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(IcedColor::from_rgb(0.14, 0.14, 0.18))),
            ..Default::default()
        })
    }

    fn tool_button(&self, tool: Tool, label: &'static str) -> impl Widget<Message> + use<'_> {
        let active = self.tool == tool;
        button(
            text(if active {
                format!("[{}]", label)
            } else {
                label.to_string()
            })
            .size(12),
        )
        .on_press(Message::SetTool(tool))
    }

    fn length_label(&self, len: NoteLength) -> String {
        if self.note_length == len {
            format!("[{}]", len.label())
        } else {
            len.label().to_string()
        }
    }

    fn build_track_list(&self) -> impl Widget<Message> + use<'_> {
        let mut items: Vec<Element<'_, Message>> = Vec::with_capacity(TRACK_COUNT);
        for i in 0..TRACK_COUNT {
            let track = &self.sequence.tracks[i];
            let (r, g, b) = TRACK_COLORS[i];
            let color_swatch = text("█")
                .color(IcedColor::from_rgb(r, g, b))
                .size(14);
            let header = row![
                color_swatch,
                text(format!("{:>2} ", i + 1)).size(12),
                text(track.name.clone()).size(12),
                Space::new(),
                button(if track.mute { "M*" } else { "M" })
                    .on_press(Message::ToggleMute(i))
                    .padding(2),
                button(if track.solo { "S*" } else { "S" })
                    .on_press(Message::ToggleSolo(i))
                    .padding(2),
                button("✎").on_press(Message::CycleTrackName(i)).padding(2),
            ]
            .spacing(4)
            .align_y(Alignment::Center);

            let selected = i == self.active;
            let strip = column![
                header,
                text(format!(
                    "ch{} prog{:>3} vol{:>3} pan{:>3} · {} notes",
                    track.channel,
                    track.program,
                    track.volume,
                    track.pan,
                    track.notes.len()
                ))
                .size(10),
            ]
            .spacing(2)
            .padding(4);

            let card = button(strip)
                .on_press(Message::SelectTrack(i))
                .width(Length::Fill)
                .padding(0)
                .style(move |_theme: &iced::Theme, status| {
                    let alpha = if selected { 0.30 } else { 0.10 };
                    let base = match status {
                        iced::widget::button::Status::Hovered => 0.45,
                        _ => alpha,
                    };
                    iced::widget::button::Style {
                        background: Some(iced::Background::Color(IcedColor::from_rgba(r, g, b, base))),
                        text_color: IcedColor::WHITE,
                        ..Default::default()
                    }
                });
            items.push(card.boxed());
        }

        let list = column(items).spacing(2).width(Length::Fill);
        Scrollable::new(
            container(list)
                .padding(6)
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .width(Length::Fixed(240.0))
        .height(Length::Fill)
    }

    fn build_editor(&self) -> impl Widget<Message> + use<'_> {
        let plot = iced_liveplot::widget::plot(&self.program).map(Message::Plot);
        let plot_area = container(plot)
            .width(Length::Fill)
            .height(Length::FillPortion(3));
        // Track bar sits directly BELOW the plot: axis mode, time signature,
        // snap division and the playhead read-out / scrub position.
        let track_bar = self.build_track_bar();
        let roll = self.build_piano_roll();
        let controls = self.build_track_controls();
        column![plot_area, track_bar, roll, controls]
            .spacing(4)
            .padding(6)
            .width(Length::Fill)
            .height(Length::Fill)
    }

    /// The strip under the plot: X-axis mode toggle, musical time signature,
    /// snap division and transport position.
    fn build_track_bar(&self) -> impl Widget<Message> + use<'_> {
        let (num, den) = self.sequence.time_signature;
        let sig_button = button(
            text(format!("Time: {}/{}", num, den)).size(12),
        )
        .on_press(Message::CycleTimeSignature);
        let axis = button(
            text(format!("X: {}", self.time_axis.label())).size(12),
        )
        .on_press(Message::ToggleTimeAxis);
        let playhead = text(format!(
            "▶ {} ticks · {}s",
            self.program.playhead_ticks(),
            self.program.playhead_seconds(),
        ))
        .size(12);

        let row = row![
            text("Track").size(14),
            rule::vertical(1),
            sig_button,
            text(format!("{}/{}", num, den)).size(11),
            rule::vertical(1),
            axis,
            rule::vertical(1),
            button(text(if self.snap_enabled { "Snap ON" } else { "Snap OFF" }).size(12))
                .on_press(Message::ToggleSnap),
            button("1/4").on_press(Message::SetSnapDivision(4)),
            button("1/8").on_press(Message::SetSnapDivision(8)),
            button("1/16").on_press(Message::SetSnapDivision(16)),
            button("1/32").on_press(Message::SetSnapDivision(32)),
            Space::new(),
            playhead,
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        container(row)
            .width(Length::Fill)
            .padding([4, 10])
            .style(|_theme: &iced::Theme| container::Style {
                background: Some(iced::Background::Color(IcedColor::from_rgb(0.14, 0.14, 0.18))),
                border: iced::Border {
                    color: IcedColor::from_rgb(0.25, 0.25, 0.30),
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Default::default()
            })
    }

    /// Keyboard strip at the bottom: labels for the visible key range.
    fn build_piano_roll(&self) -> impl Widget<Message> + use<'_> {
        let items: Vec<Element<'_, Message>> = (KEY_RANGE.0..=KEY_RANGE.1)
            .rev()
            .filter(|p| p % 12 == 0)
            .map(|pitch| {
                let black = is_black_key(pitch);
                let label = midi_note_name(pitch);
                let (r, g, b) = TRACK_COLORS[self.active];
                let color = if black {
                    IcedColor::from_rgba(r * 0.2, g * 0.2, b * 0.2, 1.0)
                } else {
                    IcedColor::from_rgba(r * 0.4, g * 0.4, b * 0.4, 1.0)
                };
                button(text(label).size(9).color(IcedColor::WHITE))
                    .width(Length::Fixed(28.0))
                    .padding(2)
                    .style(move |_theme: &iced::Theme, _s| iced::widget::button::Style {
                        background: Some(iced::Background::Color(color)),
                        ..Default::default()
                    })
                    .boxed()

            })
            .collect();
        Scrollable::new(
            row(items)
                .spacing(1)
                .padding(4)
                .width(Length::Fill)
                .height(Length::Fixed(28.0)),
        )
        .height(Length::Fixed(32.0))
        .width(Length::Fill)
    }

    fn build_track_controls(&self) -> impl Widget<Message> + use<'_> {
        let track = &self.sequence.tracks[self.active];
        let channel = track.channel;
        let program = track.program;
        let volume = track.volume;
        let pan = track.pan;

        let header = row![
            text(format!("Track {} — {}", self.active + 1, track.name)).size(16),
            Space::new(),
            text(format!("ch {} · prog {} · vol {} · pan {}", channel, program, volume, pan)).size(12),
        ]
        .spacing(8);

        // Slider closures capture the track index at build time — fine here
        // because the editor is rebuilt on every track switch.
        let active = self.active;
        let prog_slider = slider(0..=127, program as i64, move |v| Message::SetProgram(active, v))
            .width(Length::Fixed(120.0));
        let vol_slider = slider(0..=127, volume as i64, move |v| Message::SetVolume(active, v))
            .width(Length::Fixed(120.0));
        let pan_slider = slider(0..=127, pan as i64, move |v| Message::SetPan(active, v))
            .width(Length::Fixed(120.0));
        let vel_slider =
            slider(0..=127, 100u8 as i64, Message::SetVelocity).width(Length::Fixed(120.0));
        let tempo_slider = slider(40.0..=300.0, self.tempo, Message::SetTempo)
            .width(Length::Fixed(140.0))
            .step(1.0);

        let snap_row = row![
            button(if self.sequence.loop_start.is_some() { "[Loop ON]" } else { "[Loop OFF]" })
                .on_press(Message::ToggleLoop),
        ]
        .spacing(6);

        let cc_row = row![
            text("CC (track.cc_data):").size(12),
            {
                // Render the MIDI CC controllers of this track compactly.
                let mut entries: Vec<(u8, usize)> = track
                    .cc_data
                    .iter()
                    .map(|(cc, pts)| (*cc, pts.len()))
                    .collect();
                entries.sort();
                if entries.is_empty() {
                    text(" none").size(12)
                } else {
                    text(
                        entries
                            .iter()
                            .map(|(cc, n)| format!("CC{}:{}pts", cc, n))
                            .collect::<Vec<_>>()
                            .join("  "),
                    )
                    .size(12)
                }
            },
        ]
        .spacing(8);

        let help = text(
            "Grab: pan/dbl-click reset · Pen: click to insert note (length from toolbar) · \
             Move: drag note (or grab edge rectangle to resize) · Select: rubber band → group edit · \
             Erase: click to delete · Keys: G/V/B/E/M/R/T/P, Ctrl+Z/Y/C/X/V/A, Space=play",
        )
        .size(10);

        container(
            column![
                header,
                row![
                    text("Program").size(12).width(Length::Fixed(50.0)),
                    prog_slider,
                    text("Volume").size(12).width(Length::Fixed(44.0)),
                    vol_slider,
                    text("Pan").size(12).width(Length::Fixed(28.0)),
                    pan_slider,
                    text("Velocity").size(12).width(Length::Fixed(52.0)),
                    vel_slider,
                    text("Tempo").size(12).width(Length::Fixed(42.0)),
                    tempo_slider,
                    text(format!("{:.0} BPM", self.tempo)).size(12),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
                snap_row,
                cc_row,
                help,
            ]
            .spacing(6),
        )
        .padding(8)
        .width(Length::Fill)
        .style(|_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(IcedColor::from_rgb(0.10, 0.10, 0.13))),
            ..Default::default()
        })
    }

    fn subscription(&self) -> Subscription<Message> {
        iced::time::every(std::time::Duration::from_millis(32)).map(|_| Message::Tick)
    }
}

fn main() -> iced::Result {
    iced::application(SequencerApp::new, SequencerApp::update, SequencerApp::view)
        .subscription(SequencerApp::subscription)
        .window_size(iced::Size::new(1440.0, 900.0))
        .run()
}
