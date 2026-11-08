//! Automation Domain Module for iced-liveplot
//!
//! Provides automation lane data structures and rendering for parameter automation.

use crate::geom::Point;
use crate::region::{DataRegion, CurveType, SerializableColor};
use iced::{Point as IcedPoint};
use iced::widget::canvas::{LineCap, Path, Stroke, Fill, Text};
use crate::transform::Transform;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Automation point/node
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutomationPoint {
    /// Time in seconds
    pub time: f64,
    /// Parameter value (normalized 0.0-1.0 or actual range)
    pub value: f64,
    /// Interpolation curve to next point
    pub curve: CurveType,
    /// Selected state
    pub selected: bool,
}

impl AutomationPoint {
    pub fn new(time: f64, value: f64) -> Self {
        Self {
            time,
            value,
            curve: CurveType::Linear,
            selected: false,
        }
    }
    
    pub fn with_curve(time: f64, value: f64, curve: CurveType) -> Self {
        Self {
            time,
            value,
            curve,
            selected: false,
        }
    }
}

/// Automation lane for a single parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationLane {
    /// Parameter identifier (e.g., "volume", "filter_cutoff", "reverb_mix")
    pub parameter: String,
    /// Display name
    pub name: String,
    /// Unit (e.g., "dB", "Hz", "%", "semitones")
    pub unit: String,
    /// Value range (min, max)
    pub range: (f64, f64),
    /// Default value
    pub default_value: f64,
    /// Automation points (sorted by time)
    pub points: Vec<AutomationPoint>,
    /// Lane color (serializable)
    pub color: SerializableColor,
    /// Visible state
    pub visible: bool,
    /// Locked state
    pub locked: bool,
    /// Lane height (relative)
    pub height: f32,
    /// Show value labels
    pub show_labels: bool,
    /// Metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

impl AutomationLane {
    pub fn new(parameter: String, name: String, unit: String, range: (f64, f64), default: f64) -> Self {
        Self {
            parameter,
            name,
            unit,
            range,
            default_value: default,
            points: Vec::new(),
            color: SerializableColor::new(1.0, 0.6, 0.2, 1.0),
            visible: true,
            locked: false,
            height: 1.0,
            show_labels: true,
            metadata: HashMap::new(),
        }
    }
    
    /// Insert a point maintaining time order
    pub fn insert_point(&mut self, point: AutomationPoint) -> usize {
        let idx = self.points.partition_point(|p| p.time < point.time);
        self.points.insert(idx, point);
        idx
    }
    
    /// Insert point at time with value
    pub fn insert(&mut self, time: f64, value: f64) -> usize {
        self.insert_point(AutomationPoint::new(time, value.clamp(self.range.0, self.range.1)))
    }
    
    /// Insert point with curve
    pub fn insert_with_curve(&mut self, time: f64, value: f64, curve: CurveType) -> usize {
        self.insert_point(AutomationPoint::with_curve(time, value.clamp(self.range.0, self.range.1), curve))
    }
    
    /// Remove point at index
    pub fn remove_point(&mut self, index: usize) -> Option<AutomationPoint> {
        if index < self.points.len() {
            Some(self.points.remove(index))
        } else {
            None
        }
    }
    
    /// Remove points in time range
    pub fn remove_points_in_range(&mut self, start: f64, end: f64) -> Vec<AutomationPoint> {
        let mut removed = Vec::new();
        self.points.retain(|p| {
            if p.time >= start && p.time < end {
                removed.push(p.clone());
                false
            } else {
                true
            }
        });
        removed
    }
    
    /// Get interpolated value at time
    pub fn value_at(&self, time: f64) -> f64 {
        if self.points.is_empty() {
            return self.default_value;
        }
        
        // Find surrounding points
        let mut left_idx = None;
        let mut right_idx = None;
        
        for (i, point) in self.points.iter().enumerate() {
            if point.time <= time {
                left_idx = Some(i);
            } else {
                right_idx = Some(i);
                break;
            }
        }
        
        match (left_idx, right_idx) {
            (Some(l), Some(r)) => {
                let left = &self.points[l];
                let right = &self.points[r];
                
                if left.time == right.time {
                    return left.value;
                }
                
                let t = (time - left.time) / (right.time - left.time);
                left.curve.interpolate(t, left.value, right.value)
            }
            (Some(l), None) => self.points[l].value,
            (None, Some(r)) => self.points[r].value,
            (None, None) => self.default_value,
        }
    }
    
    /// Get points in time range
    pub fn points_in_range(&self, start: f64, end: f64) -> Vec<&AutomationPoint> {
        self.points.iter()
            .filter(|p| p.time >= start && p.time <= end)
            .collect()
    }
    
    /// Convert to DataRegions for rendering
    pub fn to_regions(&self) -> Vec<DataRegion> {
        self.points.iter().map(|p| {
            DataRegion::automation_point(
                self.parameter.clone(),
                p.value,
                p.time,
                p.curve,
            )
        }).collect()
    }
    
    /// Simplify automation (reduce points within tolerance)
    pub fn simplify(&mut self, tolerance: f64) {
        if self.points.len() < 3 {
            return;
        }
        
        let mut simplified = Vec::new();
        simplified.push(self.points[0].clone());
        
        for i in 1..self.points.len() - 1 {
            let prev = &simplified[simplified.len() - 1];
            let curr = &self.points[i];
            let next = &self.points[i + 1];
            
            // Check if current point is necessary
            let t = (curr.time - prev.time) / (next.time - prev.time);
            let interpolated = prev.curve.interpolate(t, prev.value, next.value);
            
            if (curr.value - interpolated).abs() > tolerance {
                simplified.push(curr.clone());
            }
        }
        
        simplified.push(self.points[self.points.len() - 1].clone());
        self.points = simplified;
    }
    
    /// Scale all values
    pub fn scale_values(&mut self, factor: f64, center: f64) {
        for point in &mut self.points {
            point.value = center + (point.value - center) * factor;
            point.value = point.value.clamp(self.range.0, self.range.1);
        }
    }
    
    /// Offset all values
    pub fn offset_values(&mut self, offset: f64) {
        for point in &mut self.points {
            point.value = (point.value + offset).clamp(self.range.0, self.range.1);
        }
    }
    
    /// Offset all times
    pub fn offset_times(&mut self, offset: f64) {
        for point in &mut self.points {
            point.time = (point.time + offset).max(0.0);
        }
        self.points.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
    }
}

/// Automation rendering style
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationStyle {
    /// Background color
    pub background: SerializableColor,
    /// Grid line color
    pub grid_line: SerializableColor,
    /// Automation line color
    pub line_color: SerializableColor,
    /// Automation line width
    pub line_width: f32,
    /// Point fill color
    pub point_fill: SerializableColor,
    /// Point stroke color
    pub point_stroke: SerializableColor,
    /// Point radius
    pub point_radius: f32,
    /// Selected point fill
    pub selected_point_fill: SerializableColor,
    /// Selected point stroke
    pub selected_point_stroke: SerializableColor,
    /// Selected point radius
    pub selected_point_radius: f32,
    /// Curve preview color
    pub curve_preview: SerializableColor,
    /// Value label color
    pub label_color: SerializableColor,
    /// Show grid
    pub show_grid: bool,
    /// Show value labels
    pub show_labels: bool,
}

impl Default for AutomationStyle {
    fn default() -> Self {
        Self {
            background: SerializableColor::new(0.12, 0.12, 0.14, 1.0),
            grid_line: SerializableColor::new(0.3, 0.3, 0.35, 0.3),
            line_color: SerializableColor::new(1.0, 0.6, 0.2, 1.0),
            line_width: 2.0,
            point_fill: SerializableColor::new(1.0, 0.6, 0.2, 1.0),
            point_stroke: SerializableColor::new(1.0, 1.0, 1.0, 1.0),
            point_radius: 4.0,
            selected_point_fill: SerializableColor::new(1.0, 0.8, 0.2, 1.0),
            selected_point_stroke: SerializableColor::new(1.0, 1.0, 1.0, 1.0),
            selected_point_radius: 6.0,
            curve_preview: SerializableColor::new(1.0, 0.6, 0.2, 0.5),
            label_color: SerializableColor::new(0.8, 0.8, 0.85, 1.0),
            show_grid: true,
            show_labels: true,
        }
    }
}

/// Render automation lane to canvas frame
pub fn render_automation_lane(
    frame: &mut iced::widget::canvas::Frame,
    lane: &AutomationLane,
    transform: &Transform,
    visible_range: crate::region::Range,
    style: &AutomationStyle,
    value_range: Option<(f64, f64)>, // Override lane range for display
) {
    let bounds = frame.size();
    let (val_min, val_max) = value_range.unwrap_or(lane.range);
    
    // Draw background
    let bg_rect = Path::rectangle(IcedPoint::ORIGIN, bounds);
    frame.fill(&bg_rect, Fill::from(style.background.to_iced()));
    
    // Draw grid lines
    if style.show_grid {
        let time_interval = (visible_range.max - visible_range.min) / 8.0;
        for i in 0..=8 {
            let t = visible_range.min + i as f64 * time_interval;
            let x = transform.data_to_screen(Point::new(t, 0.0)).unwrap().x;
            let line = Path::line(
                IcedPoint::new(x, 0.0),
                IcedPoint::new(x, bounds.height),
            );
            frame.stroke(&line, Stroke::default().with_color(style.grid_line.to_iced()).with_width(0.5));
        }
        
        let value_interval = (val_max - val_min) / 4.0;
        for i in 0..=4 {
            let v = val_min + i as f64 * value_interval;
            let y = transform.data_to_screen(Point::new(0.0, v)).unwrap().y;
            let line = Path::line(
                IcedPoint::new(0.0, y),
                IcedPoint::new(bounds.width, y),
            );
            frame.stroke(&line, Stroke::default().with_color(style.grid_line.to_iced()).with_width(0.5));
        }
    }
    
    // Draw default value line
    let default_y = transform.data_to_screen(Point::new(0.0, lane.default_value)).unwrap().y;
    let default_line = Path::line(
        IcedPoint::new(0.0, default_y),
        IcedPoint::new(bounds.width, default_y),
    );
    frame.stroke(&default_line, Stroke::default().with_color(style.grid_line.to_iced()).with_width(0.5).with_line_cap(LineCap::Round));
    
    if lane.points.is_empty() {
        // Draw label only
        if style.show_labels {
            let text = Text {
                content: format!("{} ({})", lane.name, lane.unit),
                position: iced::Point::new(8.0, 8.0),
                color: style.label_color.to_iced(),
                size: iced::Pixels(12.0),
                ..Default::default()
            };
            frame.fill_text(text);
        }
        return;
    }
    
    // Build automation curve path using builder pattern
    let mut curve_points = Vec::new();
    
    // Sample the curve at high resolution for smooth rendering
    let samples = 200;
    for i in 0..=samples {
        let t = visible_range.min + (visible_range.max - visible_range.min) * i as f64 / samples as f64;
        let v = lane.value_at(t);
        if let Some(screen) = transform.data_to_screen(Point::new(t, v)) {
            curve_points.push(IcedPoint::new(screen.x, screen.y));
        }
    }
    
    // Create path using builder
    let curve_path = if curve_points.len() >= 2 {
        let mut builder = iced::widget::canvas::path::Builder::new();
        builder.move_to(curve_points[0]);
        for pt in &curve_points[1..] {
            builder.line_to(*pt);
        }
        builder.build()
    } else {
        let mut builder = iced::widget::canvas::path::Builder::new();
        builder.build()
    };
    
    // Draw curve
    frame.stroke(&curve_path, Stroke::default().with_color(lane.color.to_iced()).with_width(style.line_width));
    
    // Draw points
    for point in &lane.points {
        if point.time < visible_range.min || point.time > visible_range.max {
            continue;
        }
        
        if let Some(screen) = transform.data_to_screen(Point::new(point.time, point.value)) {
            let radius = if point.selected { style.selected_point_radius } else { style.point_radius };
            let fill = if point.selected { style.selected_point_fill.to_iced() } else { style.point_fill.to_iced() };
            let stroke = if point.selected { style.selected_point_stroke.to_iced() } else { style.point_stroke.to_iced() };
            
            let circle = Path::circle(IcedPoint::new(screen.x, screen.y), radius);
            frame.fill(&circle, Fill::from(fill));
            frame.stroke(&circle, Stroke::default().with_color(stroke).with_width(1.5));
        }
    }
    
    // Draw labels
    if style.show_labels && lane.show_labels {
        for point in &lane.points {
            if point.time < visible_range.min || point.time > visible_range.max {
                continue;
            }
            
            if let Some(screen) = transform.data_to_screen(Point::new(point.time, point.value)) {
                let text = Text {
                    content: format!("{:.2} {}", point.value, lane.unit),
                    position: IcedPoint::new(screen.x + 8.0, screen.y - 8.0),
                    color: style.label_color.to_iced(),
                    size: iced::Pixels(10.0),
                    ..Default::default()
                };
                frame.fill_text(text);
            }
        }
    }
    
    // Draw lane name
    if style.show_labels {
        let text = Text {
            content: format!("{} ({})", lane.name, lane.unit),
            position: IcedPoint::new(8.0, 8.0),
            color: lane.color.to_iced(),
            size: iced::Pixels(12.0),
            ..Default::default()
        };
        frame.fill_text(text);
    }
}

/// Automation track containing multiple lanes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationTrack {
    pub name: String,
    pub lanes: Vec<AutomationLane>,
    pub visible: bool,
    pub height: f32,
}

impl Default for AutomationTrack {
    fn default() -> Self {
        Self {
            name: "Automation".to_string(),
            lanes: Vec::new(),
            visible: true,
            height: 100.0,
        }
    }
}

impl AutomationTrack {
    pub fn new(name: String) -> Self {
        Self {
            name,
            ..Default::default()
        }
    }
    
    pub fn add_lane(&mut self, lane: AutomationLane) {
        self.lanes.push(lane);
    }
    
    pub fn get_lane(&self, parameter: &str) -> Option<&AutomationLane> {
        self.lanes.iter().find(|l| l.parameter == parameter)
    }
    
    pub fn get_lane_mut(&mut self, parameter: &str) -> Option<&mut AutomationLane> {
        self.lanes.iter_mut().find(|l| l.parameter == parameter)
    }
    
    pub fn remove_lane(&mut self, parameter: &str) -> bool {
        let len = self.lanes.len();
        self.lanes.retain(|l| l.parameter != parameter);
        self.lanes.len() != len
    }
}

/// Common automation parameter definitions
pub mod parameters {
    use super::*;
    
    pub fn volume() -> AutomationLane {
        AutomationLane::new(
            "volume".to_string(),
            "Volume".to_string(),
            "dB".to_string(),
            (-60.0, 12.0),
            0.0,
        )
    }
    
    pub fn pan() -> AutomationLane {
        let mut lane = AutomationLane::new(
            "pan".to_string(),
            "Pan".to_string(),
            "%".to_string(),
            (-100.0, 100.0),
            0.0,
        );
        lane.color = SerializableColor::new(0.2, 0.8, 0.6, 1.0);
        lane
    }
    
    pub fn filter_cutoff() -> AutomationLane {
        let mut lane = AutomationLane::new(
            "filter_cutoff".to_string(),
            "Filter Cutoff".to_string(),
            "Hz".to_string(),
            (20.0, 20000.0),
            1000.0,
        );
        lane.color = SerializableColor::new(1.0, 0.4, 0.6, 1.0);
        lane
    }
    
    pub fn filter_resonance() -> AutomationLane {
        let mut lane = AutomationLane::new(
            "filter_resonance".to_string(),
            "Filter Resonance".to_string(),
            "".to_string(),
            (0.0, 1.0),
            0.0,
        );
        lane.color = SerializableColor::new(1.0, 0.6, 0.4, 1.0);
        lane
    }
    
    pub fn reverb_mix() -> AutomationLane {
        let mut lane = AutomationLane::new(
            "reverb_mix".to_string(),
            "Reverb Mix".to_string(),
            "%".to_string(),
            (0.0, 100.0),
            0.0,
        );
        lane.color = SerializableColor::new(0.6, 0.4, 1.0, 1.0);
        lane
    }
    
    pub fn delay_feedback() -> AutomationLane {
        let mut lane = AutomationLane::new(
            "delay_feedback".to_string(),
            "Delay Feedback".to_string(),
            "%".to_string(),
            (0.0, 100.0),
            0.0,
        );
        lane.color = SerializableColor::new(0.4, 1.0, 0.6, 1.0);
        lane
    }
    
    pub fn pitch() -> AutomationLane {
        let mut lane = AutomationLane::new(
            "pitch".to_string(),
            "Pitch".to_string(),
            "semitones".to_string(),
            (-24.0, 24.0),
            0.0,
        );
        lane.color = SerializableColor::new(1.0, 1.0, 0.4, 1.0);
        lane
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_automation_lane() {
        let mut lane = AutomationLane::new(
            "volume".to_string(),
            "Volume".to_string(),
            "dB".to_string(),
            (-60.0, 12.0),
            0.0,
        );
        
        lane.insert(0.0, 0.0);
        lane.insert(1.0, -6.0);
        lane.insert(2.0, 0.0);
        
        assert_eq!(lane.points.len(), 3);
        
        // Test interpolation
        let v = lane.value_at(0.5);
        assert!((v - (-3.0)).abs() < 0.01); // Linear interpolation
        
        let v = lane.value_at(1.5);
        assert!((v - (-3.0)).abs() < 0.01);
    }
    
    #[test]
    fn test_curve_types() {
        let mut lane = AutomationLane::new(
            "test".to_string(),
            "Test".to_string(),
            "".to_string(),
            (0.0, 1.0),
            0.0,
        );
        
        lane.insert_with_curve(0.0, 0.0, CurveType::Linear);
        lane.insert_with_curve(1.0, 1.0, CurveType::Linear);
        
        assert!((lane.value_at(0.5) - 0.5).abs() < 0.01);
        
        // Test step curve
        lane.points[0].curve = CurveType::Step;
        assert!((lane.value_at(0.5) - 0.0).abs() < 0.01);
        assert!((lane.value_at(1.0) - 1.0).abs() < 0.01);
    }
    
    #[test]
    fn test_simplify() {
        let mut lane = AutomationLane::new(
            "test".to_string(),
            "Test".to_string(),
            "".to_string(),
            (0.0, 1.0),
            0.0,
        );
        
        // Linear points - middle should be removed
        lane.insert(0.0, 0.0);
        lane.insert(0.5, 0.5);
        lane.insert(1.0, 1.0);
        
        lane.simplify(0.01);
        
        // Middle point should be removed (on the line)
        assert_eq!(lane.points.len(), 2);
    }
    
    #[test]
    fn test_common_parameters() {
        let vol = parameters::volume();
        assert_eq!(vol.parameter, "volume");
        assert_eq!(vol.range, (-60.0, 12.0));
        
        let pan = parameters::pan();
        assert_eq!(pan.parameter, "pan");
        assert_eq!(pan.range, (-100.0, 100.0));
    }
}