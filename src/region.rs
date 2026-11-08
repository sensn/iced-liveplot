//! Region System for iced-liveplot
//!
//! Provides a domain-agnostic region hierarchy supporting pins, ranges, rectangles,
//! and domain-specific regions (MIDI notes, audio regions, automation points, etc.)

use crate::geom::Point;
use crate::series::{Series, SeriesId};
use iced::Color;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::{Hash, Hasher};

/// Serializable color wrapper
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SerializableColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl SerializableColor {
    pub fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }
    
    pub fn to_iced(&self) -> Color {
        Color::from_rgba(self.r, self.g, self.b, self.a)
    }
    
    pub fn from_iced(color: Color) -> Self {
        Self {
            r: color.r,
            g: color.g,
            b: color.b,
            a: color.a,
        }
    }
    
    pub const WHITE: Self = Self { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
    pub const BLACK: Self = Self { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
    pub const TRANSPARENT: Self = Self { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };
}

impl Default for SerializableColor {
    fn default() -> Self {
        Self::new(1.0, 1.0, 1.0, 1.0)
    }
}

/// Unique identifier for a region
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RegionId(pub u64);

impl RegionId {
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        RegionId(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for RegionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Range type for regions
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Range {
    pub min: f64,
    pub max: f64,
}

impl Range {
    pub fn new(min: f64, max: f64) -> Self {
        Self { min, max }
    }
    
    pub fn contains(&self, value: f64) -> bool {
        value >= self.min && value <= self.max
    }
    
    pub fn length(&self) -> f64 {
        self.max - self.min
    }
    
    pub fn center(&self) -> f64 {
        (self.min + self.max) / 2.0
    }
    
    pub fn intersects(&self, other: &Range) -> bool {
        self.min <= other.max && self.max >= other.min
    }
}

/// Region kinds - domain-agnostic base types with domain-specific extensions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RegionKind {
    /// Generic range region (e.g., selection, loop, marker range)
    Range { x: Range, y: Option<Range> },
    
    /// Generic rectangle region
    Rectangle { x: Range, y: Range },
    
    /// Single point marker (legacy Pin equivalent)
    Point { x: f64, y: f64 },
    
    // ===== MIDI/Audio Domain Specific =====
    
    /// MIDI note region
    MidiNote { 
        pitch: u8, 
        velocity: u8, 
        channel: u8, 
        start: f64, 
        end: f64 
    },
    
    /// Audio region/clip
    AudioRegion { 
        source: AudioSource,
        start: f64, 
        duration: f64, 
        gain: f64,
        fade_in: f64,
        fade_out: f64,
        pitch_shift: f64,
        time_stretch: f64,
    },
    
    /// Automation point/node
    AutomationPoint { 
        parameter: String, 
        value: f64, 
        time: f64, 
        curve: CurveType 
    },
    
    /// Marker/label at a specific time
    Marker { 
        label: String, 
        time: f64, 
        color: SerializableColor 
    },
    
    /// Loop region
    Loop { 
        start: f64, 
        end: f64 
    },
    
    /// Punch-in/out region
    Punch { 
        start: f64, 
        end: f64 
    },
    
    /// Custom/extensible region for plugins
    Custom {
        kind: String,
        data: HashMap<String, serde_json::Value>,
    },
}

/// Audio source for audio regions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AudioSource {
    File { path: String, channel: usize },
    Buffer { 
        #[serde(skip)]
        samples: Option<std::sync::Arc<Vec<f32>>>, 
        sample_rate: u32,
        buffer_id: Option<u64>,
    },
    Generator { 
        kind: GeneratorKind, 
        params: HashMap<String, f64> 
    },
}

/// Generator types for procedural audio
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GeneratorKind {
    Sine,
    Square,
    Sawtooth,
    Triangle,
    WhiteNoise,
    PinkNoise,
}

/// Interpolation curve types for automation
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum CurveType {
    Linear,
    Exponential,
    Logarithmic,
    SCurve,
    Step,
    Bezier { 
        cp1_x: f64, cp1_y: f64, 
        cp2_x: f64, cp2_y: f64 
    },
}

impl CurveType {
    /// Interpolate between two values
    pub fn interpolate(&self, t: f64, v0: f64, v1: f64) -> f64 {
        match self {
            CurveType::Linear => v0 + t * (v1 - v0),
            CurveType::Exponential => v0 * (v1 / v0).powf(t),
            CurveType::Logarithmic => v0 + (v1 - v0) * t.ln() / 1.0_f64.ln(),
            CurveType::SCurve => {
                let t = t * t * (3.0 - 2.0 * t);
                v0 + t * (v1 - v0)
            },
            CurveType::Step => if t < 1.0 { v0 } else { v1 },
            CurveType::Bezier { cp1_x, cp1_y, cp2_x, cp2_y } => {
                // Simplified cubic bezier interpolation
                let u = 1.0 - t;
                let tt = t * t;
                let uu = u * u;
                let uuu = uu * u;
                let ttt = tt * t;
                
                uuu * 0.0 + 
                3.0 * uu * t * *cp1_x + 
                3.0 * u * tt * *cp2_x + 
                ttt * 1.0
            }
        }
    }
}

/// Visual style for a region
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionStyle {
    pub fill: Option<SerializableColor>,
    pub stroke: Option<SerializableColor>,
    pub stroke_width: f32,
    pub label: Option<String>,
    pub label_position: LabelPosition,
    pub label_color: Option<SerializableColor>,
}

impl Default for RegionStyle {
    fn default() -> Self {
        Self {
            fill: None,
            stroke: None,
            stroke_width: 1.0,
            label: None,
            label_position: LabelPosition::Center,
            label_color: None,
        }
    }
}

/// Label position for region labels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LabelPosition {
    TopLeft,
    TopCenter,
    TopRight,
    Center,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

/// Selection handles for regions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SelectionHandle {
    None,
    Move,
    ResizeLeft,
    ResizeRight,
    ResizeTop,
    ResizeBottom,
    ResizeTopLeft,
    ResizeTopRight,
    ResizeBottomLeft,
    ResizeBottomRight,
    Rotate,
}

/// Complete region data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataRegion {
    pub id: RegionId,
    pub kind: RegionKind,
    pub series: Option<SeriesId>,  // Associated series (if any)
    pub style: RegionStyle,
    pub metadata: HashMap<String, serde_json::Value>, // Extensible
    pub locked: bool,
    pub selected: bool,
    pub visible: bool,
    pub z_index: i32, // For layering
}

impl DataRegion {
    /// Create a simple point region (pin equivalent)
    pub fn point(x: f64, y: f64) -> Self {
        Self {
            id: RegionId::new(),
            kind: RegionKind::Point { x, y },
            series: None,
            style: RegionStyle::default(),
            metadata: HashMap::new(),
            locked: false,
            selected: false,
            visible: true,
            z_index: 0,
        }
    }
    
    /// Create a range region
    pub fn range(x: Range, y: Option<Range>) -> Self {
        Self {
            id: RegionId::new(),
            kind: RegionKind::Range { x, y },
            series: None,
            style: RegionStyle::default(),
            metadata: HashMap::new(),
            locked: false,
            selected: false,
            visible: true,
            z_index: 0,
        }
    }
    
    /// Create a rectangle region
    pub fn rectangle(x: Range, y: Range) -> Self {
        Self {
            id: RegionId::new(),
            kind: RegionKind::Rectangle { x, y },
            series: None,
            style: RegionStyle::default(),
            metadata: HashMap::new(),
            locked: false,
            selected: false,
            visible: true,
            z_index: 0,
        }
    }
    
    /// Create a MIDI note region
    pub fn midi_note(pitch: u8, velocity: u8, channel: u8, start: f64, end: f64) -> Self {
        Self {
            id: RegionId::new(),
            kind: RegionKind::MidiNote { pitch, velocity, channel, start, end },
            series: None,
            style: RegionStyle {
                fill: Some(SerializableColor::new(0.4, 0.6, 1.0, 0.7)),
                stroke: Some(SerializableColor::new(0.2, 0.4, 0.8, 1.0)),
                stroke_width: 1.0,
                label: None,
                label_position: LabelPosition::Center,
                label_color: Some(SerializableColor::WHITE),
            },
            metadata: HashMap::new(),
            locked: false,
            selected: false,
            visible: true,
            z_index: 10,
        }
    }
    
    /// Create an audio region
    pub fn audio_region(source: AudioSource, start: f64, duration: f64) -> Self {
        Self {
            id: RegionId::new(),
            kind: RegionKind::AudioRegion { 
                source, 
                start, 
                duration, 
                gain: 1.0,
                fade_in: 0.0,
                fade_out: 0.0,
                pitch_shift: 0.0,
                time_stretch: 1.0,
            },
            series: None,
            style: RegionStyle {
                fill: Some(SerializableColor::new(0.6, 0.8, 0.4, 0.6)),
                stroke: Some(SerializableColor::new(0.3, 0.6, 0.2, 1.0)),
                stroke_width: 1.0,
                label: None,
                label_position: LabelPosition::TopLeft,
                label_color: Some(SerializableColor::WHITE),
            },
            metadata: HashMap::new(),
            locked: false,
            selected: false,
            visible: true,
            z_index: 5,
        }
    }
    
    /// Create an automation point region
    pub fn automation_point(parameter: String, value: f64, time: f64, curve: CurveType) -> Self {
        Self {
            id: RegionId::new(),
            kind: RegionKind::AutomationPoint { parameter, value, time, curve },
            series: None,
            style: RegionStyle {
                fill: Some(SerializableColor::new(1.0, 0.6, 0.2, 0.9)),
                stroke: Some(SerializableColor::new(0.8, 0.4, 0.1, 1.0)),
                stroke_width: 2.0,
                label: None,
                label_position: LabelPosition::TopCenter,
                label_color: Some(SerializableColor::WHITE),
            },
            metadata: HashMap::new(),
            locked: false,
            selected: false,
            visible: true,
            z_index: 15,
        }
    }
    
    /// Create a marker region
    pub fn marker(label: String, time: f64, color: SerializableColor) -> Self {
        Self {
            id: RegionId::new(),
            kind: RegionKind::Marker { label: label.clone(), time, color },
            series: None,
            style: RegionStyle {
                fill: Some(color),
                stroke: Some(SerializableColor::WHITE),
                stroke_width: 2.0,
                label: Some(label),
                label_position: LabelPosition::TopCenter,
                label_color: Some(SerializableColor::WHITE),
            },
            metadata: HashMap::new(),
            locked: false,
            selected: false,
            visible: true,
            z_index: 20,
        }
    }
    
    /// Create a loop region
    pub fn loop_region(start: f64, end: f64) -> Self {
        Self {
            id: RegionId::new(),
            kind: RegionKind::Loop { start, end },
            series: None,
            style: RegionStyle {
                fill: Some(SerializableColor::new(1.0, 0.8, 0.0, 0.3)),
                stroke: Some(SerializableColor::new(1.0, 0.6, 0.0, 1.0)),
                stroke_width: 2.0,
                label: Some("Loop".to_string()),
                label_position: LabelPosition::TopLeft,
                label_color: Some(SerializableColor::new(1.0, 0.6, 0.0, 1.0)),
            },
            metadata: HashMap::new(),
            locked: false,
            selected: false,
            visible: true,
            z_index: 1,
        }
    }
    
    /// Get the time range of this region (if applicable)
    pub fn time_range(&self) -> Option<Range> {
        match &self.kind {
            RegionKind::Range { x, .. } => Some(*x),
            RegionKind::Rectangle { x, .. } => Some(*x),
            RegionKind::Point { x, .. } => Some(Range::new(*x, *x)),
            RegionKind::MidiNote { start, end, .. } => Some(Range::new(*start, *end)),
            RegionKind::AudioRegion { start, duration, .. } => Some(Range::new(*start, start + duration)),
            RegionKind::AutomationPoint { time, .. } => Some(Range::new(*time, *time)),
            RegionKind::Marker { time, .. } => Some(Range::new(*time, *time)),
            RegionKind::Loop { start, end } => Some(Range::new(*start, *end)),
            RegionKind::Punch { start, end } => Some(Range::new(*start, *end)),
            RegionKind::Custom { .. } => None,
        }
    }
    
    /// Get the value range of this region (if applicable)
    pub fn value_range(&self) -> Option<Range> {
        match &self.kind {
            RegionKind::Range { y, .. } => *y,
            RegionKind::Rectangle { y, .. } => Some(*y),
            RegionKind::Point { y, .. } => Some(Range::new(*y, *y)),
            RegionKind::MidiNote { pitch, .. } => Some(Range::new(*pitch as f64, *pitch as f64 + 1.0)),
            RegionKind::AudioRegion { .. } => None,
            RegionKind::AutomationPoint { value, .. } => Some(Range::new(*value, *value)),
            RegionKind::Marker { .. } => None,
            RegionKind::Loop { .. } => None,
            RegionKind::Punch { .. } => None,
            RegionKind::Custom { .. } => None,
        }
    }
    
    /// Check if a point (in data coordinates) is inside this region
    pub fn contains(&self, x: f64, y: f64) -> bool {
        match &self.kind {
            RegionKind::Range { x: xr, y: yr } => {
                xr.contains(x) && yr.map_or(true, |yr| yr.contains(y))
            }
            RegionKind::Rectangle { x: xr, y: yr } => {
                xr.contains(x) && yr.contains(y)
            }
            RegionKind::Point { x: px, y: py } => {
                (*px - x).abs() < 0.001 && (*py - y).abs() < 0.001
            }
            RegionKind::MidiNote { start, end, pitch, .. } => {
                x >= *start && x <= *end && y >= *pitch as f64 && y < *pitch as f64 + 1.0
            }
            RegionKind::AudioRegion { start, duration, .. } => {
                x >= *start && x <= start + duration
            }
            RegionKind::AutomationPoint { time, value, .. } => {
                (x - *time).abs() < 0.001 && (y - *value).abs() < 0.001
            }
            RegionKind::Marker { time, .. } => {
                (x - *time).abs() < 0.001
            }
            RegionKind::Loop { start, end } => {
                x >= *start && x <= *end
            }
            RegionKind::Punch { start, end } => {
                x >= *start && x <= *end
            }
            RegionKind::Custom { .. } => false,
        }
    }
    
    /// Get the bounds in screen coordinates (requires transform)
    pub fn screen_bounds(&self, transform: &crate::transform::Transform) -> Option<crate::geom::ScreenRect> {
        let time_range = self.time_range()?;
        let value_range = self.value_range().unwrap_or(Range::new(0.0, 1.0));
        
        let p1 = transform.data_to_screen(Point::new(time_range.min, value_range.min));
        let p2 = transform.data_to_screen(Point::new(time_range.max, value_range.max));
        
        let Some(p1) = p1 else { return None };
        let Some(p2) = p2 else { return None };
        
        let min_x = p1.x.min(p2.x);
        let max_x = p1.x.max(p2.x);
        let min_y = p1.y.min(p2.y);
        let max_y = p1.y.max(p2.y);
        
        Some(crate::geom::ScreenRect::new(
            crate::geom::ScreenPoint::new(min_x, min_y),
            crate::geom::ScreenPoint::new(max_x, max_y),
        ))
    }
}

/// Layer for managing multiple regions
#[derive(Debug, Default, Clone)]
pub struct RegionLayer {
    regions: Vec<DataRegion>,
    spatial_index: Option<SpatialIndex>,
    dirty: bool,
}

impl RegionLayer {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn add(&mut self, region: DataRegion) -> RegionId {
        let id = region.id;
        self.regions.push(region);
        self.dirty = true;
        id
    }
    
    pub fn remove(&mut self, id: RegionId) -> bool {
        let len = self.regions.len();
        self.regions.retain(|r| r.id != id);
        self.dirty = self.regions.len() != len;
        self.dirty
    }
    
    pub fn get(&self, id: RegionId) -> Option<&DataRegion> {
        self.regions.iter().find(|r| r.id == id)
    }
    
    pub fn get_mut(&mut self, id: RegionId) -> Option<&mut DataRegion> {
        self.regions.iter_mut().find(|r| r.id == id)
    }
    
    pub fn all(&self) -> &[DataRegion] {
        &self.regions
    }
    
    pub fn all_mut(&mut self) -> &mut [DataRegion] {
        &mut self.regions
    }
    
    pub fn visible(&self) -> Vec<&DataRegion> {
        self.regions.iter().filter(|r| r.visible).collect()
    }
    
    pub fn selected(&self) -> Vec<&DataRegion> {
        self.regions.iter().filter(|r| r.selected).collect()
    }
    
    pub fn selected_mut(&mut self) -> Vec<&mut DataRegion> {
        self.regions.iter_mut().filter(|r| r.selected).collect()
    }
    
    pub fn clear_selection(&mut self) {
        for region in &mut self.regions {
            region.selected = false;
        }
    }
    
    pub fn select(&mut self, id: RegionId) -> bool {
        if let Some(r) = self.regions.iter_mut().find(|r| r.id == id) {
            r.selected = true;
            true
        } else {
            false
        }
    }
    
    pub fn deselect(&mut self, id: RegionId) -> bool {
        if let Some(r) = self.regions.iter_mut().find(|r| r.id == id) {
            r.selected = false;
            true
        } else {
            false
        }
    }
    
    pub fn hit_test(&self, x: f64, y: f64, transform: &crate::transform::Transform) -> Option<RegionId> {
        // Test in reverse z-order (top-most first)
        let mut sorted: Vec<_> = self.regions.iter().filter(|r| r.visible).collect();
        sorted.sort_by_key(|r| -r.z_index);
        
        for region in sorted {
            if region.contains(x, y) {
                return Some(region.id);
            }
        }
        None
    }
    
    pub fn hit_test_rect(&self, rect: crate::geom::ScreenRect, transform: &crate::transform::Transform) -> Vec<RegionId> {
        let mut results = Vec::new();
        
        // Convert screen rect to data coordinates
        let p1 = transform.screen_to_data(crate::geom::ScreenPoint::new(rect.min.x, rect.min.y));
        let p2 = transform.screen_to_data(crate::geom::ScreenPoint::new(rect.max.x, rect.max.y));
        
        let (p1, p2) = match (p1, p2) {
            (Some(p1), Some(p2)) => (p1, p2),
            _ => return results,
        };
        
        let x_range = Range::new(p1.x.min(p2.x), p1.x.max(p2.x));
        let y_range = Range::new(p1.y.min(p2.y), p1.y.max(p2.y));
        
        for region in &self.regions {
            if !region.visible {
                continue;
            }
            
            if let Some(tr) = region.time_range() {
                if tr.intersects(&x_range) {
                    if let Some(vr) = region.value_range() {
                        if vr.intersects(&y_range) {
                            results.push(region.id);
                        }
                    } else {
                        results.push(region.id);
                    }
                }
            }
        }
        
        results
    }
    
    pub fn len(&self) -> usize {
        self.regions.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }
    
    pub fn clear(&mut self) {
        self.regions.clear();
        self.dirty = true;
    }
}

/// Simple spatial index for faster hit testing (placeholder for future optimization)
#[derive(Debug, Default, Clone)]
struct SpatialIndex {
    // Could be a quadtree, R-tree, or grid-based index
    // For now, we just use linear search
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_region_creation() {
        let point = DataRegion::point(1.0, 2.0);
        assert!(matches!(point.kind, RegionKind::Point { x: 1.0, y: 2.0 }));
        
        let range = DataRegion::range(Range::new(0.0, 10.0), Some(Range::new(-1.0, 1.0)));
        assert!(matches!(range.kind, RegionKind::Range { .. }));
        
        let midi = DataRegion::midi_note(60, 100, 0, 1.0, 2.0);
        assert!(matches!(midi.kind, RegionKind::MidiNote { pitch: 60, .. }));
    }
    
    #[test]
    fn test_region_contains() {
        let point = DataRegion::point(1.0, 2.0);
        assert!(point.contains(1.0, 2.0));
        assert!(!point.contains(1.1, 2.0));
        
        let range = DataRegion::range(Range::new(0.0, 10.0), Some(Range::new(-1.0, 1.0)));
        assert!(range.contains(5.0, 0.0));
        assert!(!range.contains(15.0, 0.0));
        assert!(!range.contains(5.0, 2.0));
        
        let midi = DataRegion::midi_note(60, 100, 0, 1.0, 2.0);
        assert!(midi.contains(1.5, 60.5));
        assert!(!midi.contains(2.5, 60.5));
        assert!(!midi.contains(1.5, 61.5));
    }
    
    #[test]
    fn test_region_layer() {
        let mut layer = RegionLayer::new();
        
        let id1 = layer.add(DataRegion::point(1.0, 2.0));
        let id2 = layer.add(DataRegion::point(3.0, 4.0));
        
        assert_eq!(layer.len(), 2);
        assert!(layer.get(id1).is_some());
        assert!(layer.get(id2).is_some());
        
        layer.remove(id1);
        assert_eq!(layer.len(), 1);
        assert!(layer.get(id1).is_none());
        assert!(layer.get(id2).is_some());
    }
}