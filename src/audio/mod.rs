//! Audio Domain Module for iced-liveplot
//!
//! Provides audio region/clip data structures and waveform rendering.

use crate::geom::Point;
use crate::region::{DataRegion, RegionId, RegionKind, AudioSource, GeneratorKind, RegionStyle, LabelPosition, SerializableColor};
use iced::{Color, Point as IcedPoint};
use crate::transform::Transform;
use crate::datasource::DecimationScratch;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Audio region/clip representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioRegion {
    /// Unique identifier
    pub id: RegionId,
    /// Audio source
    pub source: AudioSource,
    /// Timeline position (seconds)
    pub start: f64,
    /// Duration (seconds)
    pub duration: f64,
    /// Gain multiplier (1.0 = unity)
    pub gain: f64,
    /// Fade in duration (seconds)
    pub fade_in: f64,
    /// Fade out duration (seconds)
    pub fade_out: f64,
    /// Pitch shift in semitones
    pub pitch_shift: f64,
    /// Time stretch ratio (1.0 = normal)
    pub time_stretch: f64,
    /// Region style
    pub style: RegionStyle,
    /// Locked state
    pub locked: bool,
    /// Selected state
    pub selected: bool,
    /// Visible state
    pub visible: bool,
    /// Z-index for layering
    pub z_index: i32,
    /// Metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

impl AudioRegion {
    pub fn new(source: AudioSource, start: f64, duration: f64) -> Self {
        Self {
            id: RegionId::new(),
            source,
            start,
            duration,
            gain: 1.0,
            fade_in: 0.0,
            fade_out: 0.0,
            pitch_shift: 0.0,
            time_stretch: 1.0,
            style: RegionStyle {
                fill: Some(SerializableColor::new(0.6, 0.8, 0.4, 0.6)),
                stroke: Some(SerializableColor::new(0.3, 0.6, 0.2, 1.0)),
                stroke_width: 1.0,
                label: None,
                label_position: LabelPosition::TopLeft,
                label_color: Some(SerializableColor::WHITE),
            },
            locked: false,
            selected: false,
            visible: true,
            z_index: 5,
            metadata: HashMap::new(),
        }
    }
    
    /// End time in seconds
    pub fn end(&self) -> f64 {
        self.start + self.duration
    }
    
    /// Convert to DataRegion for generic rendering
    pub fn to_data_region(&self) -> DataRegion {
        DataRegion {
            id: self.id,
            kind: RegionKind::AudioRegion {
                source: self.source.clone(),
                start: self.start,
                duration: self.duration,
                gain: self.gain,
                fade_in: self.fade_in,
                fade_out: self.fade_out,
                pitch_shift: self.pitch_shift,
                time_stretch: self.time_stretch,
            },
            series: None,
            style: self.style.clone(),
            metadata: self.metadata.clone(),
            locked: self.locked,
            selected: self.selected,
            visible: self.visible,
            z_index: self.z_index,
        }
    }
    
    /// Get gain envelope value at time (relative to region start)
    pub fn gain_at(&self, local_time: f64) -> f64 {
        let mut gain = self.gain;
        
        // Fade in
        if local_time < self.fade_in && self.fade_in > 0.0 {
            gain *= local_time / self.fade_in;
        }
        
        // Fade out
        if local_time > self.duration - self.fade_out && self.fade_out > 0.0 {
            gain *= (self.duration - local_time) / self.fade_out;
        }
        
        gain.clamp(0.0, 10.0)
    }
    
    /// Split region at time (returns new region for the right portion)
    pub fn split(&mut self, time: f64) -> Option<AudioRegion> {
        if time <= self.start || time >= self.end() {
            return None;
        }
        
        let right_duration = self.end() - time;
        self.duration = time - self.start;
        
        let mut right = self.clone();
        right.id = RegionId::new();
        right.start = time;
        right.duration = right_duration;
        right.fade_in = 0.0; // No fade in on split
        
        Some(right)
    }
    
    /// Apply fade in
    pub fn apply_fade_in(&mut self, duration: f64) {
        self.fade_in = duration.min(self.duration / 2.0);
    }
    
    /// Apply fade out
    pub fn apply_fade_out(&mut self, duration: f64) {
        self.fade_out = duration.min(self.duration / 2.0);
    }
    
    /// Reverse the region (requires buffer source)
    pub fn reverse(&mut self) -> bool {
        // This would require reversing the underlying buffer
        // For now, just mark it
        self.metadata.insert("reversed".to_string(), serde_json::Value::Bool(true));
        true
    }
}

/// Waveform rendering style
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveformStyle {
    /// Background color
    pub background: SerializableColor,
    /// Waveform fill color (positive)
    pub fill_positive: SerializableColor,
    /// Waveform fill color (negative)
    pub fill_negative: SerializableColor,
    /// Waveform outline color
    pub outline: SerializableColor,
    /// Center line color
    pub center_line: SerializableColor,
    /// Gain envelope color
    pub envelope: SerializableColor,
    /// Selected waveform fill
    pub selected_fill: SerializableColor,
    /// Selected waveform outline
    pub selected_outline: SerializableColor,
    /// Show filled area
    pub fill: bool,
    /// Show outline
    pub outline_width: f32,
    /// Center line width
    pub center_line_width: f32,
    /// Minimum height in pixels
    pub min_height: f32,
    /// Gain envelope line width
    pub envelope_width: f32,
}

impl Default for WaveformStyle {
    fn default() -> Self {
        Self {
            background: SerializableColor::new(0.12, 0.12, 0.14, 1.0),
            fill_positive: SerializableColor::new(0.4, 0.7, 0.4, 0.6),
            fill_negative: SerializableColor::new(0.7, 0.4, 0.4, 0.6),
            outline: SerializableColor::new(0.3, 0.6, 0.3, 1.0),
            center_line: SerializableColor::new(0.5, 0.5, 0.55, 0.8),
            envelope: SerializableColor::new(1.0, 0.8, 0.2, 0.8),
            selected_fill: SerializableColor::new(0.6, 0.9, 0.6, 0.8),
            selected_outline: SerializableColor::new(0.2, 0.8, 0.2, 1.0),
            fill: true,
            outline_width: 1.0,
            center_line_width: 0.5,
            min_height: 2.0,
            envelope_width: 1.5,
        }
    }
}

/// Render waveform to canvas frame
pub fn render_waveform(
    frame: &mut iced::widget::canvas::Frame,
    region: &AudioRegion,
    transform: &Transform,
    visible_range: crate::region::Range,
    decimation: &mut DecimationScratch,
    style: &WaveformStyle,
) {
    use iced::widget::canvas::{Fill, Stroke, Path};
    
    let bounds = frame.size();
    
    // Get visible portion of region
    let region_start = region.start.max(visible_range.min);
    let region_end = region.end().min(visible_range.max);
    
    if region_start >= region_end {
        return;
    }
    
    // Fetch audio data for visible range
    let samples = match fetch_audio_samples(region, region_start, region_end) {
        Some(s) => s,
        None => return,
    };
    
    if samples.is_empty() {
        return;
    }
    
    // Decimate to screen width
    let screen_width = bounds.width;
    let decimated = decimate_minmax(&samples, screen_width as usize);
    
    if decimated.is_empty() {
        return;
    }
    
    // Calculate vertical center and scale
    let y_center = transform.data_to_screen(Point::new(0.0, 0.0)).unwrap().y;
    let y_scale = (transform.data_to_screen(Point::new(0.0, -1.0)).unwrap().y - y_center).abs();
    
    // Draw center line
    let center_line = Path::line(
        IcedPoint::new(0.0, y_center),
        IcedPoint::new(bounds.width, y_center),
    );
    frame.stroke(&center_line, Stroke::default().with_color(style.center_line.to_iced()).with_width(style.center_line_width));
    
    // Draw waveform
    let x_step = bounds.width / decimated.len() as f32;
    
    // Build path for filled area
    if style.fill {
        let mut points = Vec::new();
        
        // Top edge (positive)
        for (i, &(min, max)) in decimated.iter().enumerate() {
            let x = i as f32 * x_step;
            let y = y_center - max as f32 * y_scale;
            points.push(IcedPoint::new(x, y));
        }
        
        // Bottom edge (negative) - reversed
        for (i, &(min, _max)) in decimated.iter().enumerate().rev() {
            let x = i as f32 * x_step;
            let y = y_center - min as f32 * y_scale;
            points.push(IcedPoint::new(x, y));
        }
        
        let path = if points.len() >= 3 {
            let mut builder = iced::widget::canvas::path::Builder::new();
            builder.move_to(points[0]);
            for pt in &points[1..] {
                builder.line_to(*pt);
            }
            builder.close();
            builder.build()
        } else {
            let mut builder = iced::widget::canvas::path::Builder::new();
            builder.build()
        };
        
        let fill_color = if region.selected { style.selected_fill.to_iced() } else { style.fill_positive.to_iced() };
        frame.fill(&path, Fill::from(fill_color));
    }
    
    // Draw outline
    if style.outline_width > 0.0 {
        let mut top_points = Vec::new();
        let mut bottom_points = Vec::new();
        
        for (i, &(min, max)) in decimated.iter().enumerate() {
            let x = i as f32 * x_step;
            let y_top = y_center - max as f32 * y_scale;
            let y_bottom = y_center - min as f32 * y_scale;
            
            top_points.push(IcedPoint::new(x, y_top));
            bottom_points.push(IcedPoint::new(x, y_bottom));
        }
        
        let top_path = if top_points.len() >= 2 {
            let mut builder = iced::widget::canvas::path::Builder::new();
            builder.move_to(top_points[0]);
            for pt in &top_points[1..] {
                builder.line_to(*pt);
            }
            builder.build()
        } else {
            let mut builder = iced::widget::canvas::path::Builder::new();
            builder.build()
        };
        
        let bottom_path = if bottom_points.len() >= 2 {
            let mut builder = iced::widget::canvas::path::Builder::new();
            builder.move_to(bottom_points[0]);
            for pt in &bottom_points[1..] {
                builder.line_to(*pt);
            }
            builder.build()
        } else {
            let mut builder = iced::widget::canvas::path::Builder::new();
            builder.build()
        };
        
        let outline_color = if region.selected { style.selected_outline.to_iced() } else { style.outline.to_iced() };
        frame.stroke(&top_path, Stroke::default().with_color(outline_color).with_width(style.outline_width));
        frame.stroke(&bottom_path, Stroke::default().with_color(outline_color).with_width(style.outline_width));
    }
    
    // Draw gain envelope
    if region.fade_in > 0.0 || region.fade_out > 0.0 {
        let mut env_points = Vec::new();
        
        for i in 0..decimated.len() {
            let x = i as f32 * x_step;
            let local_time = region_start + (i as f64 / decimated.len() as f64) * (region_end - region_start) - region.start;
            let gain = region.gain_at(local_time);
            let y = y_center - gain as f32 * y_scale;
            
            env_points.push(IcedPoint::new(x, y));
        }
        
        let env_path = if env_points.len() >= 2 {
            let mut builder = iced::widget::canvas::path::Builder::new();
            builder.move_to(env_points[0]);
            for pt in &env_points[1..] {
                builder.line_to(*pt);
            }
            builder.build()
        } else {
            let mut builder = iced::widget::canvas::path::Builder::new();
            builder.build()
        };
        
        frame.stroke(&env_path, Stroke::default().with_color(style.envelope.to_iced()).with_width(style.envelope_width));
        
        // Mirror envelope (negative)
        let mut env_points_neg = Vec::new();
        for i in 0..decimated.len() {
            let x = i as f32 * x_step;
            let local_time = region_start + (i as f64 / decimated.len() as f64) * (region_end - region_start) - region.start;
            let gain = region.gain_at(local_time);
            let y = y_center + gain as f32 * y_scale;
            
            env_points_neg.push(IcedPoint::new(x, y));
        }
        
        let env_path_neg = if env_points_neg.len() >= 2 {
            let mut builder = iced::widget::canvas::path::Builder::new();
            builder.move_to(env_points_neg[0]);
            for pt in &env_points_neg[1..] {
                builder.line_to(*pt);
            }
            builder.build()
        } else {
            let mut builder = iced::widget::canvas::path::Builder::new();
            builder.build()
        };
        frame.stroke(&env_path_neg, Stroke::default().with_color(style.envelope.to_iced()).with_width(style.envelope_width));
    }
}

/// Fetch audio samples for a region and time range
fn fetch_audio_samples(region: &AudioRegion, start: f64, end: f64) -> Option<Vec<f32>> {
    match &region.source {
        AudioSource::Buffer { samples, sample_rate, .. } => {
            if let Some(buf) = samples {
                let start_sample = ((start - region.start) * *sample_rate as f64) as usize;
                let end_sample = ((end - region.start) * *sample_rate as f64) as usize;
                let start_sample = start_sample.min(buf.len());
                let end_sample = end_sample.min(buf.len());
                
                if start_sample < end_sample {
                    Some(buf[start_sample..end_sample].to_vec())
                } else {
                    Some(Vec::new())
                }
            } else {
                None
            }
        }
        AudioSource::File { .. } => {
            // Would need to load from file
            // For now, return dummy data
            let duration = end - start;
            let sample_count = (duration * 44100.0) as usize;
            Some(vec![0.0; sample_count])
        }
        AudioSource::Generator { kind, params } => {
            generate_audio(kind, params, start - region.start, end - start)
        }
    }
}

/// Generate audio from a generator
fn generate_audio(kind: &GeneratorKind, params: &HashMap<String, f64>, start: f64, end: f64) -> Option<Vec<f32>> {
    let sample_rate = params.get("sample_rate").copied().unwrap_or(44100.0) as u32;
    let frequency = params.get("frequency").copied().unwrap_or(440.0);
    let amplitude = params.get("amplitude").copied().unwrap_or(0.5);
    
    let start_sample = (start * sample_rate as f64) as usize;
    let end_sample = (end * sample_rate as f64) as usize;
    let len = end_sample - start_sample;
    
    let mut samples = Vec::with_capacity(len);
    
    for i in 0..len {
        let t = (start_sample + i) as f64 / sample_rate as f64;
        let value = match kind {
            GeneratorKind::Sine => (t * frequency * 2.0 * std::f64::consts::PI).sin(),
            GeneratorKind::Square => {
                if (t * frequency * 2.0 * std::f64::consts::PI).sin() >= 0.0 { 1.0 } else { -1.0 }
            }
            GeneratorKind::Sawtooth => 2.0 * (t * frequency - (t * frequency + 0.5).floor()),
            GeneratorKind::Triangle => 2.0 * (2.0 * (t * frequency - (t * frequency + 0.5).floor()).abs() - 1.0).abs(),
            GeneratorKind::WhiteNoise => rand::random::<f64>() * 2.0 - 1.0,
            GeneratorKind::PinkNoise => {
                // Simplified pink noise
                rand::random::<f64>() * 2.0 - 1.0
            }
        };
        samples.push((value * amplitude) as f32);
    }
    
    Some(samples)
}

/// Min/max decimation for waveform rendering
fn decimate_minmax(samples: &[f32], target_len: usize) -> Vec<(f32, f32)> {
    if samples.len() <= target_len * 2 {
        // Just return min/max pairs
        return samples.chunks(2)
            .map(|chunk| {
                let min = chunk.iter().cloned().fold(f32::INFINITY, f32::min);
                let max = chunk.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                (min, max)
            })
            .collect();
    }
    
    let chunk_size = samples.len() / target_len;
    let mut result = Vec::with_capacity(target_len);
    
    for i in 0..target_len {
        let start = i * chunk_size;
        let end = ((i + 1) * chunk_size).min(samples.len());
        if start < end {
            let chunk = &samples[start..end];
            let min = chunk.iter().cloned().fold(f32::INFINITY, f32::min);
            let max = chunk.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            result.push((min, max));
        }
    }
    
    result
}

/// Audio track containing multiple regions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioTrack {
    pub name: String,
    pub regions: Vec<AudioRegion>,
    pub volume: f64,
    pub pan: f64,
    pub mute: bool,
    pub solo: bool,
    pub color: SerializableColor,
}

impl Default for AudioTrack {
    fn default() -> Self {
        Self {
            name: "Audio Track".to_string(),
            regions: Vec::new(),
            volume: 1.0,
            pan: 0.0,
            mute: false,
            solo: false,
            color: SerializableColor::new(0.6, 0.8, 0.4, 1.0),
        }
    }
}

impl AudioTrack {
    pub fn new(name: String) -> Self {
        Self {
            name,
            ..Default::default()
        }
    }
    
    pub fn add_region(&mut self, region: AudioRegion) {
        self.regions.push(region);
        self.regions.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());
    }
    
    pub fn remove_region(&mut self, id: RegionId) -> bool {
        let len = self.regions.len();
        self.regions.retain(|r| r.id != id);
        self.regions.len() != len
    }
    
    pub fn regions_in_range(&self, start: f64, end: f64) -> Vec<&AudioRegion> {
        self.regions.iter()
            .filter(|r| r.start < end && r.end() > start)
            .collect()
    }
    
    pub fn total_duration(&self) -> f64 {
        self.regions.iter()
            .map(|r| r.end())
            .fold(0.0, f64::max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_audio_region() {
        let source = AudioSource::Buffer {
            samples: Some(Arc::new(vec![0.0; 44100])),
            sample_rate: 44100,
            buffer_id: None,
        };
        
        let mut region = AudioRegion::new(source, 0.0, 1.0);
        assert_eq!(region.start, 0.0);
        assert_eq!(region.duration, 1.0);
        assert_eq!(region.end(), 1.0);
        
        region.apply_fade_in(0.1);
        region.apply_fade_out(0.1);
        assert_eq!(region.fade_in, 0.1);
        assert_eq!(region.fade_out, 0.1);
    }
    
    #[test]
    fn test_gain_envelope() {
        let source = AudioSource::Buffer {
            samples: Some(Arc::new(vec![1.0; 44100])),
            sample_rate: 44100,
            buffer_id: None,
        };
        
        let mut region = AudioRegion::new(source, 0.0, 1.0);
        region.fade_in = 0.5;
        region.fade_out = 0.5;
        region.gain = 1.0;
        
        assert!((region.gain_at(0.0) - 0.0).abs() < 0.01);
        assert!((region.gain_at(0.25) - 0.5).abs() < 0.01);
        assert!((region.gain_at(0.5) - 1.0).abs() < 0.01);
        assert!((region.gain_at(0.75) - 0.5).abs() < 0.01);
        assert!((region.gain_at(1.0) - 0.0).abs() < 0.01);
    }
    
    #[test]
    fn test_split() {
        let source = AudioSource::Buffer {
            samples: Some(Arc::new(vec![0.0; 44100])),
            sample_rate: 44100,
            buffer_id: None,
        };
        
        let mut region = AudioRegion::new(source, 0.0, 2.0);
        let right = region.split(1.0);
        
        assert!(right.is_some());
        assert_eq!(region.duration, 1.0);
        assert_eq!(right.unwrap().duration, 1.0);
    }
    
    #[test]
    fn test_decimate() {
        let samples = vec![1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0];
        let decimated = decimate_minmax(&samples, 4);
        
        assert_eq!(decimated.len(), 4);
        for (min, max) in decimated {
            assert_eq!(min, -1.0);
            assert_eq!(max, 1.0);
        }
    }
}