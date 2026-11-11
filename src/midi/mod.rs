//! MIDI Domain Module for iced-liveplot
//!
//! Provides MIDI-specific data structures and rendering for piano roll visualization.

use crate::geom::Point;
use crate::region::{DataRegion, RegionId, RegionKind, AudioSource, GeneratorKind, CurveType, RegionStyle, LabelPosition, SerializableColor};
use crate::series::{Series, SeriesId, SeriesKind};
use iced::{Point as IcedPoint};
use crate::transform::Transform;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// MIDI note representation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MidiNote {
    /// MIDI pitch (0-127, middle C = 60)
    pub pitch: u8,
    /// MIDI velocity (0-127)
    pub velocity: u8,
    /// MIDI channel (0-15)
    pub channel: u8,
    /// Start time in ticks
    pub start_tick: u64,
    /// End time in ticks
    pub end_tick: u64,
}

impl MidiNote {
    pub fn new(pitch: u8, velocity: u8, channel: u8, start_tick: u64, end_tick: u64) -> Self {
        Self {
            pitch,
            velocity,
            channel,
            start_tick,
            end_tick,
        }
    }
    
    /// Duration in ticks
    pub fn duration_ticks(&self) -> u64 {
        self.end_tick.saturating_sub(self.start_tick)
    }
    
    /// Convert to data region for rendering
    pub fn to_region(&self, ticks_per_beat: u64, tempo_bpm: f64) -> DataRegion {
        let start = self.tick_to_seconds(self.start_tick, ticks_per_beat, tempo_bpm);
        let end = self.tick_to_seconds(self.end_tick, ticks_per_beat, tempo_bpm);
        DataRegion::midi_note(self.pitch, self.velocity, self.channel, start, end)
    }
    
    /// Convert ticks to seconds
    pub fn tick_to_seconds(&self, tick: u64, ticks_per_beat: u64, tempo_bpm: f64) -> f64 {
        let beats = tick as f64 / ticks_per_beat as f64;
        beats * 60.0 / tempo_bpm
    }
    
    /// Convert seconds to ticks
    pub fn seconds_to_tick(seconds: f64, ticks_per_beat: u64, tempo_bpm: f64) -> u64 {
        let beats = seconds * tempo_bpm / 60.0;
        (beats * ticks_per_beat as f64).round() as u64
    }
}

/// MIDI track containing notes and CC data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiTrack {
    pub name: String,
    pub channel: u8,
    pub notes: Vec<MidiNote>,
    pub cc_data: HashMap<u8, Vec<(u64, u8)>>, // CC# -> (tick, value)
    pub program: u8,
    pub volume: u8,
    pub pan: u8,
    pub mute: bool,
    pub solo: bool,
}

impl Default for MidiTrack {
    fn default() -> Self {
        Self {
            name: "Track".to_string(),
            channel: 0,
            notes: Vec::new(),
            cc_data: HashMap::new(),
            program: 0,
            volume: 100,
            pan: 64,
            mute: false,
            solo: false,
        }
    }
}

impl MidiTrack {
    pub fn new(name: String) -> Self {
        Self {
            name,
            ..Default::default()
        }
    }
    
    /// Add a note to the track
    pub fn add_note(&mut self, note: MidiNote) {
        self.notes.push(note);
        self.notes.sort_by_key(|n| n.start_tick);
    }
    
    /// Remove a note by index
    pub fn remove_note(&mut self, index: usize) -> Option<MidiNote> {
        if index < self.notes.len() {
            Some(self.notes.remove(index))
        } else {
            None
        }
    }
    
    /// Get notes in a time range
    pub fn notes_in_range(&self, start_tick: u64, end_tick: u64) -> Vec<&MidiNote> {
        self.notes.iter()
            .filter(|n| n.start_tick < end_tick && n.end_tick > start_tick)
            .collect()
    }
    
    /// Quantize notes to grid
    pub fn quantize(&mut self, grid: u64, ticks_per_beat: u64, tempo_bpm: f64, strength: f64) {
        for note in &mut self.notes {
            let start_beats = note.start_tick as f64 / ticks_per_beat as f64;
            let grid_beats = grid as f64 / ticks_per_beat as f64;
            let quantized_beats = (start_beats / grid_beats).round() * grid_beats;
            let quantized_tick = (quantized_beats * ticks_per_beat as f64).round() as u64;
            
            let delta = quantized_tick as i64 - note.start_tick as i64;
            let adjusted_delta = (delta as f64 * strength).round() as i64;
            
            note.start_tick = (note.start_tick as i64 + adjusted_delta).max(0) as u64;
            note.end_tick = (note.end_tick as i64 + adjusted_delta).max((note.start_tick + 1) as i64) as u64;
        }
        self.notes.sort_by_key(|n| n.start_tick);
    }
    
    /// Transpose notes
    pub fn transpose(&mut self, semitones: i8) {
        for note in &mut self.notes {
            let new_pitch = note.pitch as i16 + semitones as i16;
            note.pitch = new_pitch.clamp(0, 127) as u8;
        }
    }
    
    /// Set velocity for notes in range
    pub fn set_velocity(&mut self, velocity: u8, start_tick: u64, end_tick: u64) {
        for note in &mut self.notes {
            if note.start_tick >= start_tick && note.start_tick < end_tick {
                note.velocity = velocity;
            }
        }
    }
}

/// Piano roll rendering style
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PianoRollStyle {
    /// Background color
    pub background: SerializableColor,
    /// White key color
    pub white_key: SerializableColor,
    /// Black key color
    pub black_key: SerializableColor,
    /// Note fill color
    pub note_fill: SerializableColor,
    /// Note stroke color
    pub note_stroke: SerializableColor,
    /// Selected note fill
    pub selected_note_fill: SerializableColor,
    /// Selected note stroke
    pub selected_note_stroke: SerializableColor,
    /// Velocity color gradient (low to high)
    pub velocity_gradient: Vec<SerializableColor>,
    /// Grid line color
    pub grid_line: SerializableColor,
    /// Beat line color (stronger)
    pub beat_line: SerializableColor,
    /// Bar line color (strongest)
    pub bar_line: SerializableColor,
    /// Key label color
    pub key_label: SerializableColor,
    /// Note label color
    pub note_label: SerializableColor,
    /// Show note names
    pub show_note_names: bool,
    /// Show velocity as height
    pub show_velocity: bool,
    /// Key width in pixels
    pub key_width: f32,
    /// Minimum note height in pixels
    pub min_note_height: f32,
}

impl Default for PianoRollStyle {
    fn default() -> Self {
        Self {
            background: SerializableColor::new(0.12, 0.12, 0.14, 1.0),
            white_key: SerializableColor::new(0.18, 0.18, 0.20, 1.0),
            black_key: SerializableColor::new(0.08, 0.08, 0.10, 1.0),
            note_fill: SerializableColor::new(0.4, 0.6, 1.0, 0.8),
            note_stroke: SerializableColor::new(0.2, 0.4, 0.8, 1.0),
            selected_note_fill: SerializableColor::new(1.0, 0.8, 0.2, 0.9),
            selected_note_stroke: SerializableColor::new(1.0, 0.6, 0.0, 1.0),
            velocity_gradient: vec![
                SerializableColor::new(0.3, 0.3, 0.8, 0.6),
                SerializableColor::new(0.4, 0.6, 1.0, 0.8),
                SerializableColor::new(0.6, 0.8, 1.0, 0.9),
                SerializableColor::new(0.8, 1.0, 0.8, 1.0),
                SerializableColor::new(1.0, 0.8, 0.3, 1.0),
            ],
            grid_line: SerializableColor::new(0.3, 0.3, 0.35, 0.5),
            beat_line: SerializableColor::new(0.4, 0.4, 0.45, 0.7),
            bar_line: SerializableColor::new(0.5, 0.5, 0.55, 1.0),
            key_label: SerializableColor::new(0.6, 0.6, 0.65, 1.0),
            note_label: SerializableColor::WHITE,
            show_note_names: true,
            show_velocity: true,
            key_width: 60.0,
            min_note_height: 2.0,
        }
    }
}

/// Render piano roll to a canvas frame
pub fn render_piano_roll(
    frame: &mut iced::widget::canvas::Frame,
    track: &MidiTrack,
    transform: &Transform,
    visible_range: crate::region::Range,
    ticks_per_beat: u64,
    tempo_bpm: f64,
    style: &PianoRollStyle,
    key_range: (u8, u8), // (min_pitch, max_pitch)
) {
    use iced::widget::canvas::{Fill, Stroke, Path, Text};
    use crate::geom::ScreenPoint;
    
    let bounds = frame.size();
    let key_width = style.key_width;
    let piano_left = key_width;
    
    // Draw piano key backgrounds
    for pitch in key_range.0..=key_range.1 {
        let is_black = is_black_key(pitch);
        let y_center = transform.data_to_screen(Point::new(0.0, pitch as f64 + 0.5)).unwrap().y;
        let key_height = (transform.data_to_screen(Point::new(0.0, pitch as f64)).unwrap().y - 
                         transform.data_to_screen(Point::new(0.0, pitch as f64 + 1.0)).unwrap().y).abs();
        
        let key_rect = Path::rectangle(
            iced::Point::new(0.0, y_center - key_height / 2.0),
            iced::Size::new(key_width, key_height),
        );
        
        frame.fill(&key_rect, Fill::from((if is_black { style.black_key } else { style.white_key }).to_iced()));
    }
    
    // Draw grid lines (beat/bar lines)
    let beat_interval = 60.0 / tempo_bpm; // seconds per beat
    let bar_interval = beat_interval * 4.0; // 4/4 time
    
    let start_beat = (visible_range.min / beat_interval).floor() as i64;
    let end_beat = (visible_range.max / beat_interval).ceil() as i64;
    
    for beat in start_beat..=end_beat {
        let time = beat as f64 * beat_interval;
        if time < visible_range.min || time > visible_range.max {
            continue;
        }
        
        let x = transform.data_to_screen(Point::new(time, 0.0)).unwrap().x;
        let is_bar = beat % 4 == 0;
        
        let line = Path::line(
            iced::Point::new(x, 0.0),
            iced::Point::new(x, bounds.height),
        );
        
        let color = if is_bar { style.bar_line.to_iced() } else { style.beat_line.to_iced() };
        let width = if is_bar { 2.0 } else { 1.0 };
        
        frame.stroke(&line, Stroke::default().with_color(color).with_width(width));
    }
    
    // Draw horizontal grid lines (every octave)
    for octave in 0..11 {
        let pitch = octave * 12;
        if pitch < key_range.0 || pitch > key_range.1 {
            continue;
        }
        let y = transform.data_to_screen(Point::new(0.0, pitch as f64)).unwrap().y;
        let line = Path::line(
            iced::Point::new(piano_left, y),
            iced::Point::new(bounds.width, y),
        );
        frame.stroke(&line, Stroke::default().with_color(style.grid_line.to_iced()).with_width(0.5));
    }
    
    // Draw notes
    for note in &track.notes {
        if note.pitch < key_range.0 || note.pitch > key_range.1 {
            continue;
        }
        
        let start = MidiNote::tick_to_seconds_static(note.start_tick, ticks_per_beat, tempo_bpm);
        let end = MidiNote::tick_to_seconds_static(note.end_tick, ticks_per_beat, tempo_bpm);
        
        if end < visible_range.min || start > visible_range.max {
            continue;
        }
        
        let x1 = transform.data_to_screen(Point::new(start.max(visible_range.min), 0.0)).unwrap().x;
        let x2 = transform.data_to_screen(Point::new(end.min(visible_range.max), 0.0)).unwrap().x;
        let y_center = transform.data_to_screen(Point::new(0.0, note.pitch as f64 + 0.5)).unwrap().y;
        let note_height = (transform.data_to_screen(Point::new(0.0, note.pitch as f64)).unwrap().y - 
                          transform.data_to_screen(Point::new(0.0, note.pitch as f64 + 1.0)).unwrap().y).abs();
        
        let rect = Path::rectangle(
            iced::Point::new(x1.max(piano_left), y_center - note_height / 2.0 + 1.0),
            iced::Size::new((x2 - x1).max(style.min_note_height), note_height - 2.0),
        );
        
        // Color based on velocity
        let velocity_factor = note.velocity as f32 / 127.0;
        let color_idx = (velocity_factor * (style.velocity_gradient.len() - 1) as f32) as usize;
        let fill_color = style.velocity_gradient[color_idx.min(style.velocity_gradient.len() - 1)].to_iced();
        
        frame.fill(&rect, Fill::from(fill_color));
        frame.stroke(&rect, Stroke::default().with_color(style.note_stroke.to_iced()).with_width(1.0));
        
        // Draw velocity indicator (small bar on left)
        if style.show_velocity {
            let vel_width = 3.0;
            let vel_height = note_height * velocity_factor;
            let vel_rect = Path::rectangle(
                iced::Point::new(x1.max(piano_left), y_center + note_height / 2.0 - vel_height - 1.0),
                iced::Size::new(vel_width, vel_height.max(1.0)),
            );
            frame.fill(&vel_rect, Fill::from(style.velocity_gradient.last().copied().unwrap_or(SerializableColor::WHITE).to_iced()));
        }
    }
    
    // Draw key labels
    if style.show_note_names {
        for pitch in key_range.0..=key_range.1 {
            if pitch % 12 == 0 { // Only label C notes
                let y = transform.data_to_screen(Point::new(0.0, pitch as f64 + 0.5)).unwrap().y;
                let note_name = midi_note_name(pitch);
                let text = Text {
                    content: note_name,
                    position: iced::Point::new(4.0, y - 6.0),
                    color: style.key_label.to_iced(),
                    size: iced::Pixels(11.0),
                    ..Default::default()
                };
                frame.fill_text(text);
            }
        }
    }
}

/// Check if a MIDI pitch is a black key
pub fn is_black_key(pitch: u8) -> bool {
    let note = pitch % 12;
    matches!(note, 1 | 3 | 6 | 8 | 10) // C#, D#, F#, G#, A#
}

/// Get note name for a MIDI pitch
pub fn midi_note_name(pitch: u8) -> String {
    let names = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    let octave = pitch as i16 / 12 - 1;
    format!("{}{}", names[pitch as usize % 12], octave)
}

/// MIDI sequence/track collection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiSequence {
    pub tracks: Vec<MidiTrack>,
    pub tempo: f64,
    pub time_signature: (u8, u8), // numerator, denominator
    pub ticks_per_beat: u64,
    pub loop_start: Option<u64>,
    pub loop_end: Option<u64>,
}

impl Default for MidiSequence {
    fn default() -> Self {
        Self {
            tracks: Vec::new(),
            tempo: 120.0,
            time_signature: (4, 4),
            ticks_per_beat: 480,
            loop_start: None,
            loop_end: None,
        }
    }
}

impl MidiSequence {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn add_track(&mut self, track: MidiTrack) {
        self.tracks.push(track);
    }
    
    pub fn total_duration_ticks(&self) -> u64 {
        self.tracks.iter()
            .flat_map(|t| t.notes.iter())
            .map(|n| n.end_tick)
            .max()
            .unwrap_or(0)
    }
    
    pub fn total_duration_seconds(&self) -> f64 {
        let ticks = self.total_duration_ticks();
        MidiNote::tick_to_seconds_static(ticks, self.ticks_per_beat, self.tempo)
    }

    /// Number of notes in a track (0 when out of range).
    pub fn note_count(&self, track: usize) -> usize {
        self.tracks.get(track).map(|t| t.notes.len()).unwrap_or(0)
    }

    /// Remove the note at `track[index]`, returning it.
    pub fn remove_note(&mut self, track: usize, index: usize) -> Option<MidiNote> {
        self.tracks.get_mut(track).and_then(|t| {
            if index < t.notes.len() {
                Some(t.notes.remove(index))
            } else {
                None
            }
        })
    }

    /// Paste `notes` into `track`, shifted so they start at the track's
    /// earliest gap (or keep their start when `offset_ticks` is 0). Notes are
    /// re-sorted and assigned to the track's own channel.
    pub fn paste_notes(&mut self, track: usize, notes: &[MidiNote], offset_ticks: u64) {
        let Some(track_ref) = self.tracks.get_mut(track) else { return };
        let channel = track_ref.channel;
        for mut note in notes.iter().cloned() {
            note.start_tick += offset_ticks;
            note.end_tick += offset_ticks;
            note.channel = channel;
            track_ref.add_note(note);
        }
    }

    /// Delete every note of the sequence (all tracks).
    pub fn clear_all(&mut self) {
        for track in &mut self.tracks {
            track.notes.clear();
        }
    }

    /// Ticks per quarter note (alias of `ticks_per_beat`).
    pub fn ppq(&self) -> u64 {
        self.ticks_per_beat
    }
}

impl MidiNote {
    pub fn tick_to_seconds_static(tick: u64, ticks_per_beat: u64, tempo_bpm: f64) -> f64 {
        let beats = tick as f64 / ticks_per_beat as f64;
        beats * 60.0 / tempo_bpm
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_midi_note() {
        let note = MidiNote::new(60, 100, 0, 0, 480);
        assert_eq!(note.pitch, 60);
        assert_eq!(note.duration_ticks(), 480);
        
        let region = note.to_region(480, 120.0);
        assert!(matches!(region.kind, RegionKind::MidiNote { pitch: 60, .. }));
    }
    
    #[test]
    fn test_tick_conversion() {
        let seconds = MidiNote::tick_to_seconds_static(480, 480, 120.0);
        assert!((seconds - 0.5).abs() < 0.001); // 1 beat at 120 BPM = 0.5s
        
        let tick = MidiNote::seconds_to_tick(1.0, 480, 120.0);
        assert_eq!(tick, 960); // 2 beats = 960 ticks
    }
    
    #[test]
    fn test_note_name() {
        assert_eq!(midi_note_name(60), "C4"); // Middle C
        assert_eq!(midi_note_name(69), "A4"); // A440
        assert_eq!(midi_note_name(21), "A0"); // Lowest piano key
        assert_eq!(midi_note_name(108), "C8"); // Highest piano key
    }
    
    #[test]
    fn test_black_key() {
        assert!(is_black_key(61)); // C#4
        assert!(!is_black_key(60)); // C4
        assert!(is_black_key(63)); // D#4
        assert!(!is_black_key(62)); // D4
    }
    
    #[test]
    fn test_midi_track() {
        let mut track = MidiTrack::new("Test".to_string());
        track.add_note(MidiNote::new(60, 100, 0, 0, 480));
        track.add_note(MidiNote::new(64, 80, 0, 480, 960));
        
        assert_eq!(track.notes.len(), 2);
        
        track.transpose(12);
        assert_eq!(track.notes[0].pitch, 72);
    }
    
    #[test]
    fn test_quantize() {
        let mut track = MidiTrack::new("Test".to_string());
        // Note slightly off grid
        track.add_note(MidiNote::new(60, 100, 0, 10, 490)); // Should quantize to 0
        
        track.quantize(480, 480, 120.0, 1.0); // Quantize to quarter notes
        
        assert_eq!(track.notes[0].start_tick, 0);
    }
}