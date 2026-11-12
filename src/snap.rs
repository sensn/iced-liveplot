//! Snap/Grid System for iced-liveplot
//!
//! Provides configurable snapping to grids, time bases, and musical divisions.

use crate::geom::Point;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Time base for grid/snap calculations
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TimeBase {
    /// Samples (audio sample rate)
    Samples(u32),
    /// Seconds
    Seconds,
    /// Milliseconds
    Milliseconds,
    /// Frames (video)
    Frames(u32), // fps as integer
    /// Musical beats (tempo-dependent)
    Beats,
    /// Musical bars
    Bars,
    /// MIDI ticks
    Ticks(u32), // ticks per beat
}

/// Musical grid specification
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GridSpec {
    /// Time base
    pub time_base: TimeBase,
    /// Division (e.g., 4 for quarter notes, 8 for eighth notes, 16 for sixteenth notes)
    pub division: u32,
    /// Swing factor (0.0 = straight, 0.5 = triplet feel)
    pub swing: f64,
    /// Offset in grid units
    pub offset: f64,
}

impl Default for GridSpec {
    fn default() -> Self {
        Self {
            time_base: TimeBase::Beats,
            division: 16, // Sixteenth notes
            swing: 0.0,
            offset: 0.0,
        }
    }
}

impl GridSpec {
    /// Create a musical grid (beats)
    pub fn musical(division: u32) -> Self {
        Self {
            time_base: TimeBase::Beats,
            division,
            swing: 0.0,
            offset: 0.0,
        }
    }
    
    /// Create a straight grid in seconds
    pub fn seconds(interval: f64) -> Self {
        Self {
            time_base: TimeBase::Seconds,
            division: (1.0 / interval) as u32,
            swing: 0.0,
            offset: 0.0,
        }
    }
    
    /// Create a grid for samples
    pub fn samples(sample_rate: u32, samples_per_division: u32) -> Self {
        Self {
            time_base: TimeBase::Samples(sample_rate),
            division: samples_per_division,
            swing: 0.0,
            offset: 0.0,
        }
    }
    
    /// Create a grid for frames
    pub fn frames(fps: u32, frames_per_division: u32) -> Self {
        Self {
            time_base: TimeBase::Frames(fps),
            division: frames_per_division,
            swing: 0.0,
            offset: 0.0,
        }
    }
    
    /// Get the interval in seconds for a given tempo (BPM)
    pub fn interval_seconds(&self, bpm: f64) -> f64 {
        match self.time_base {
            TimeBase::Beats => 60.0 / bpm / self.division as f64,
            TimeBase::Bars => 60.0 / bpm * 4.0 / self.division as f64,
            TimeBase::Seconds => 1.0 / self.division as f64,
            TimeBase::Milliseconds => 0.001 / self.division as f64,
            TimeBase::Frames(fps) => 1.0 / fps as f64 / self.division as f64,
            TimeBase::Samples(rate) => self.division as f64 / rate as f64,
            TimeBase::Ticks(ticks_per_beat) => 60.0 / bpm / ticks_per_beat as f64 * self.division as f64,
        }
    }
    
    /// Snap a time value to the grid
    pub fn snap_time(&self, time: f64, bpm: f64) -> f64 {
        let interval = self.interval_seconds(bpm);
        if interval <= 0.0 {
            return time;
        }
        
        let grid_pos = (time - self.offset) / interval;
        let snapped_grid_pos = if self.swing > 0.0 {
            // Apply swing: odd divisions are delayed
            let division = grid_pos.floor() as i64;
            let fraction = grid_pos.fract();
            if division % 2 == 1 {
                // Odd division - apply swing delay
                division as f64 + fraction * (1.0 - self.swing) + self.swing
            } else {
                grid_pos
            }
        } else {
            grid_pos.round()
        };
        
        snapped_grid_pos * interval + self.offset
    }
    
    /// Get grid lines in a range (for rendering)
    pub fn grid_lines(&self, range: crate::region::Range, bpm: f64) -> Vec<f64> {
        let interval = self.interval_seconds(bpm);
        if interval <= 0.0 {
            return Vec::new();
        }
        
        let start = ((range.min - self.offset) / interval).floor() as i64;
        let end = ((range.max - self.offset) / interval).ceil() as i64;
        
        (start..=end)
            .map(|i| i as f64 * interval + self.offset)
            .filter(|&t| t >= range.min && t <= range.max)
            .collect()
    }
}

/// Snap configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapConfig {
    /// Whether snapping is enabled
    pub enabled: bool,
    /// Grid specification
    pub grid: Option<GridSpec>,
    /// Time base for snap calculations
    pub time_base: TimeBase,
    /// Snap threshold in screen pixels
    pub threshold_px: f32,
    /// Snap to regions (not just grid)
    pub snap_to_regions: bool,
    /// Region types to snap to
    pub snap_region_kinds: Vec<String>,
    /// Gravity strength (0.0 - 1.0)
    pub gravity: f32,
}

impl Default for SnapConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            grid: Some(GridSpec::default()),
            time_base: TimeBase::Beats,
            threshold_px: 10.0,
            snap_to_regions: true,
            snap_region_kinds: vec!["MidiNote".to_string(), "AudioRegion".to_string(), "Marker".to_string()],
            gravity: 0.5,
        }
    }
}

impl SnapConfig {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn with_grid(mut self, grid: GridSpec) -> Self {
        self.grid = Some(grid);
        self
    }
    
    pub fn with_threshold(mut self, px: f32) -> Self {
        self.threshold_px = px;
        self
    }
    
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Default::default()
        }
    }
}

/// Snap result
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapResult {
    pub point: Point,
    pub snapped: bool,
    pub snap_type: SnapType,
    pub distance: f32,
}

/// Type of snap that occurred
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapType {
    None,
    Grid,
    RegionStart,
    RegionEnd,
    RegionCenter,
    RegionEdge,
    OtherPoint,
}

/// Snap engine
#[derive(Clone)]
pub struct SnapEngine {
    config: SnapConfig,
    /// Cached grid lines for current view
    cached_grid: Vec<f64>,
    cached_range: Option<crate::region::Range>,
    cached_bpm: f64,
}

impl SnapEngine {
    
    /// Snap a pure time position (seconds) to the configured grid.
    ///
    /// Returns `None` when snapping is disabled or no grid is configured.
    pub fn snap_position_seconds(&self, seconds: f64) -> Option<f64> {
        if !self.config.enabled {
            return None;
        }
        let grid = self.config.grid.as_ref()?;
        let beat = 60.0 / 120.0; // default tempo; tempo-aware snapping uses snap()
        let _ = beat;
        Some(grid.snap_time(seconds, 120.0))
    }

    pub fn new(config: SnapConfig) -> Self {
        Self {
            config,
            cached_grid: Vec::new(),
            cached_range: None,
            cached_bpm: 120.0,
        }
    }
    
    pub fn config(&self) -> &SnapConfig {
        &self.config
    }
    
    pub fn config_mut(&mut self) -> &mut SnapConfig {
        &mut self.config
    }
    
    /// Update cached grid lines for a view range
    pub fn update_grid_cache(&mut self, range: crate::region::Range, bpm: f64) {
        if self.cached_range != Some(range) || (self.cached_bpm - bpm).abs() > 0.001 {
            if let Some(ref grid) = self.config.grid {
                self.cached_grid = grid.grid_lines(range, bpm);
            } else {
                self.cached_grid.clear();
            }
            self.cached_range = Some(range);
            self.cached_bpm = bpm;
        }
    }
    
    /// Snap a point to the grid
    pub fn snap_to_grid(&self, point: Point, bpm: f64) -> Point {
        if let Some(ref grid) = self.config.grid {
            let snapped_x = grid.snap_time(point.x, bpm);
            Point::new(snapped_x, point.y)
        } else {
            point
        }
    }
    
    /// Snap a point to nearby regions
    pub fn snap_to_regions(&self, point: Point, regions: &[crate::region::DataRegion], transform: &crate::transform::Transform) -> SnapResult {
        if !self.config.snap_to_regions {
            return SnapResult {
                point,
                snapped: false,
                snap_type: SnapType::None,
                distance: f32::INFINITY,
            };
        }
        
        let threshold = self.config.threshold_px;
        let mut best = SnapResult {
            point,
            snapped: false,
            snap_type: SnapType::None,
            distance: f32::INFINITY,
        };
        
        for region in regions {
            if !region.visible || region.locked {
                continue;
            }
            
            // Check if region kind is in snap list
            let kind_name = region_kind_name(&region.kind);
            if !self.config.snap_region_kinds.contains(&kind_name) {
                continue;
            }
            
            // Get region bounds in data coordinates
            if let Some(tr) = region.time_range() {
                // Snap to region start
                if let (Some(start_screen), Some(point_screen)) = (
                    transform.data_to_screen(Point::new(tr.min, point.y)),
                    transform.data_to_screen(point)
                ) {
                    let dist_start = ((start_screen.x - point_screen.x).powi(2) + (start_screen.y - point_screen.y).powi(2)).sqrt();
                    
                    if dist_start < threshold && dist_start < best.distance {
                        best = SnapResult {
                            point: Point::new(tr.min, point.y),
                            snapped: true,
                            snap_type: SnapType::RegionStart,
                            distance: dist_start,
                        };
                    }
                }
                
                // Snap to region end
                if let (Some(end_screen), Some(point_screen)) = (
                    transform.data_to_screen(Point::new(tr.max, point.y)),
                    transform.data_to_screen(point)
                ) {
                    let dist_end = ((end_screen.x - point_screen.x).powi(2) + (end_screen.y - point_screen.y).powi(2)).sqrt();
                    
                    if dist_end < threshold && dist_end < best.distance {
                        best = SnapResult {
                            point: Point::new(tr.max, point.y),
                            snapped: true,
                            snap_type: SnapType::RegionEnd,
                            distance: dist_end,
                        };
                    }
                }
                
                // Snap to region center
                if let (Some(center_screen), Some(point_screen)) = (
                    transform.data_to_screen(Point::new(tr.center(), point.y)),
                    transform.data_to_screen(point)
                ) {
                    let dist_center = ((center_screen.x - point_screen.x).powi(2) + (center_screen.y - point_screen.y).powi(2)).sqrt();
                    
                    if dist_center < threshold && dist_center < best.distance {
                        best = SnapResult {
                            point: Point::new(tr.center(), point.y),
                            snapped: true,
                            snap_type: SnapType::RegionCenter,
                            distance: dist_center,
                        };
                    }
                }
            }
            
            if let Some(vr) = region.value_range() {
                // Snap to region value edges
                if let (Some(bottom_screen), Some(point_screen)) = (
                    transform.data_to_screen(Point::new(point.x, vr.min)),
                    transform.data_to_screen(point)
                ) {
                    let dist_bottom = ((bottom_screen.x - point_screen.x).powi(2) + (bottom_screen.y - point_screen.y).powi(2)).sqrt();
                    
                    if dist_bottom < threshold && dist_bottom < best.distance {
                        best = SnapResult {
                            point: Point::new(point.x, vr.min),
                            snapped: true,
                            snap_type: SnapType::RegionEdge,
                            distance: dist_bottom,
                        };
                    }
                }
                
                if let (Some(top_screen), Some(point_screen)) = (
                    transform.data_to_screen(Point::new(point.x, vr.max)),
                    transform.data_to_screen(point)
                ) {
                    let dist_top = ((top_screen.x - point_screen.x).powi(2) + (top_screen.y - point_screen.y).powi(2)).sqrt();
                    
                    if dist_top < threshold && dist_top < best.distance {
                        best = SnapResult {
                            point: Point::new(point.x, vr.max),
                            snapped: true,
                            snap_type: SnapType::RegionEdge,
                            distance: dist_top,
                        };
                    }
                }
            }
        }
        
        best
    }
    
    /// Snap a point (combines grid and region snapping)
    pub fn snap(&mut self, point: Point, regions: &[crate::region::DataRegion], transform: &crate::transform::Transform, view_range: crate::region::Range, bpm: f64) -> SnapResult {
        if !self.config.enabled {
            return SnapResult {
                point,
                snapped: false,
                snap_type: SnapType::None,
                distance: f32::INFINITY,
            };
        }
        
        self.update_grid_cache(view_range, bpm);
        
        // First try grid snap
        let grid_snapped = self.snap_to_grid(point, bpm);
        let grid_result = SnapResult {
            point: grid_snapped,
            snapped: (grid_snapped.x - point.x).abs() > 0.0001,
            snap_type: SnapType::Grid,
            distance: 0.0,
        };
        
        // Then try region snap
        let region_result = self.snap_to_regions(point, regions, transform);
        
        // Return the closer snap (with gravity)
        if region_result.snapped && grid_result.snapped {
            // Both snapped - pick closer one
            if region_result.distance < grid_result.distance * (1.0 - self.config.gravity) {
                region_result
            } else {
                grid_result
            }
        } else if region_result.snapped {
            region_result
        } else if grid_result.snapped {
            grid_result
        } else {
            SnapResult {
                point,
                snapped: false,
                snap_type: SnapType::None,
                distance: f32::INFINITY,
            }
        }
    }
    
    /// Get grid lines for rendering
    pub fn grid_lines(&self) -> &[f64] {
        &self.cached_grid
    }
}

fn region_kind_name(kind: &crate::region::RegionKind) -> String {
    match kind {
        crate::region::RegionKind::Range { .. } => "Range",
        crate::region::RegionKind::Rectangle { .. } => "Rectangle",
        crate::region::RegionKind::Point { .. } => "Point",
        crate::region::RegionKind::MidiNote { .. } => "MidiNote",
        crate::region::RegionKind::AudioRegion { .. } => "AudioRegion",
        crate::region::RegionKind::AutomationPoint { .. } => "AutomationPoint",
        crate::region::RegionKind::Marker { .. } => "Marker",
        crate::region::RegionKind::Loop { .. } => "Loop",
        crate::region::RegionKind::Punch { .. } => "Punch",
        crate::region::RegionKind::Custom { kind, .. } => kind,
    }.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_grid_spec() {
        let grid = GridSpec::musical(16);
        assert_eq!(grid.division, 16);
        assert_eq!(grid.time_base, TimeBase::Beats);
        
        let interval = grid.interval_seconds(120.0);
        assert!((interval - 0.03125).abs() < 0.001); // 1/32 second at 120 BPM
    }
    
    #[test]
    fn test_snap_time() {
        let grid = GridSpec::musical(4); // Quarter notes
        // At 120 BPM, quarter note = 0.5s
        let snapped = grid.snap_time(1.23, 120.0);
        assert!((snapped - 1.0).abs() < 0.001 || (snapped - 1.5).abs() < 0.001);
    }
    
    #[test]
    fn test_grid_lines() {
        let grid = GridSpec::seconds(1.0); // 1 second grid
        let lines = grid.grid_lines(crate::region::Range::new(0.0, 5.0), 120.0);
        assert_eq!(lines.len(), 6); // 0, 1, 2, 3, 4, 5
    }
}