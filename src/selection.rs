//! Selection Model for iced-liveplot
//!
//! Provides multi-selection support for pins, regions, and data points.

use crate::region::{DataRegion, RegionId, RegionKind};
use crate::series::{SeriesId, Series};
use crate::geom::Point;
use crate::interaction::Pin;
use std::collections::{HashMap, HashSet};

/// Selection anchor for shift-extend operations
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectionAnchor {
    pub kind: AnchorKind,
    pub screen_pos: crate::geom::ScreenPoint,
}

/// Types of selection anchors
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnchorKind {
    Pin(Pin),
    Region(RegionId),
    Point(SeriesId, usize),
}

/// Selection state
#[derive(Debug, Default, Clone)]
pub struct Selection {
    /// Selected pins
    pub pins: HashSet<Pin>,
    
    /// Selected regions
    pub regions: HashSet<RegionId>,
    
    /// Selected points per series (series_id -> set of point indices)
    pub points: HashMap<SeriesId, HashSet<usize>>,
    
    /// Anchor for shift-extend selection
    pub anchor: Option<SelectionAnchor>,
}

impl Selection {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Check if anything is selected
    pub fn is_empty(&self) -> bool {
        self.pins.is_empty() && self.regions.is_empty() && self.points.is_empty()
    }
    
    /// Get total count of selected items
    pub fn len(&self) -> usize {
        self.pins.len() + self.regions.len() + self.points.values().map(|s| s.len()).sum::<usize>()
    }
    
    /// Clear all selections
    pub fn clear(&mut self) {
        self.pins.clear();
        self.regions.clear();
        self.points.clear();
        self.anchor = None;
    }
    
    // ===== Pin Selection =====
    
    pub fn select_pin(&mut self, pin: Pin) {
        self.pins.insert(pin);
    }
    
    pub fn deselect_pin(&mut self, pin: &Pin) {
        self.pins.remove(pin);
    }
    
    pub fn toggle_pin(&mut self, pin: Pin) {
        if self.pins.contains(&pin) {
            self.pins.remove(&pin);
        } else {
            self.pins.insert(pin);
        }
    }
    
    pub fn is_pin_selected(&self, pin: &Pin) -> bool {
        self.pins.contains(pin)
    }
    
    pub fn selected_pins(&self) -> &HashSet<Pin> {
        &self.pins
    }
    
    // ===== Region Selection =====
    
    pub fn select_region(&mut self, id: RegionId) {
        self.regions.insert(id);
    }
    
    pub fn deselect_region(&mut self, id: &RegionId) {
        self.regions.remove(id);
    }
    
    pub fn toggle_region(&mut self, id: RegionId) {
        if self.regions.contains(&id) {
            self.regions.remove(&id);
        } else {
            self.regions.insert(id);
        }
    }
    
    pub fn is_region_selected(&self, id: &RegionId) -> bool {
        self.regions.contains(id)
    }
    
    pub fn selected_regions(&self) -> &HashSet<RegionId> {
        &self.regions
    }
    
    // ===== Point Selection =====
    
    pub fn select_point(&mut self, series: SeriesId, index: usize) {
        self.points.entry(series).or_default().insert(index);
    }
    
    pub fn deselect_point(&mut self, series: SeriesId, index: usize) {
        if let Some(set) = self.points.get_mut(&series) {
            set.remove(&index);
            if set.is_empty() {
                self.points.remove(&series);
            }
        }
    }
    
    pub fn toggle_point(&mut self, series: SeriesId, index: usize) {
        let set = self.points.entry(series).or_default();
        if set.contains(&index) {
            set.remove(&index);
        } else {
            set.insert(index);
        }
    }
    
    pub fn is_point_selected(&self, series: SeriesId, index: usize) -> bool {
        self.points.get(&series).map_or(false, |s| s.contains(&index))
    }
    
    pub fn selected_points(&self, series: SeriesId) -> Option<&HashSet<usize>> {
        self.points.get(&series)
    }
    
    pub fn select_points_range(&mut self, series: SeriesId, range: std::ops::Range<usize>) {
        let set = self.points.entry(series).or_default();
        for i in range {
            set.insert(i);
        }
    }
    
    pub fn deselect_points_range(&mut self, series: SeriesId, range: std::ops::Range<usize>) {
        if let Some(set) = self.points.get_mut(&series) {
            for i in range {
                set.remove(&i);
            }
            if set.is_empty() {
                self.points.remove(&series);
            }
        }
    }
    
    // ===== Anchor for Shift-Extend =====
    
    pub fn set_anchor(&mut self, anchor: SelectionAnchor) {
        self.anchor = Some(anchor);
    }
    
    pub fn clear_anchor(&mut self) {
        self.anchor = None;
    }
    
    pub fn anchor(&self) -> Option<SelectionAnchor> {
        self.anchor
    }
    
    // ===== Select All / Deselect All =====
    
    pub fn select_all_pins(&mut self, pins: &HashSet<Pin>) {
        self.pins.extend(pins.iter().cloned());
    }
    
    pub fn select_all_regions(&mut self, regions: &[RegionId]) {
        self.regions.extend(regions.iter().cloned());
    }
    
    pub fn select_all_points(&mut self, series: SeriesId, points: &[Point]) {
        let set = self.points.entry(series).or_default();
        for i in 0..points.len() {
            set.insert(i);
        }
    }
    
    // ===== Box Selection Helpers =====
    
    /// Select all pins within a screen rectangle (requires transform)
    pub fn select_pins_in_rect(
        &mut self, 
        pins: &HashSet<Pin>, 
        rect: crate::geom::ScreenRect, 
        transform: &crate::transform::Transform,
        series_data: &HashMap<SeriesId, Vec<Point>>
    ) {
        for pin in pins {
            // Get the point from the series data
            if let Some(points) = series_data.get(&pin.series) {
                if pin.index < points.len() {
                    let point = points[pin.index];
                    if let Some(screen) = transform.data_to_screen(point) {
                        if rect_contains(rect, screen) {
                            self.pins.insert(*pin);
                        }
                    }
                }
            }
        }
    }
    
    /// Select all regions intersecting a screen rectangle
    pub fn select_regions_in_rect(
        &mut self, 
        regions: &[DataRegion], 
        rect: crate::geom::ScreenRect, 
        transform: &crate::transform::Transform
    ) {
        // Convert screen rect to data coordinates
        let p1 = transform.screen_to_data(crate::geom::ScreenPoint::new(rect.min.x, rect.min.y));
        let p2 = transform.screen_to_data(crate::geom::ScreenPoint::new(rect.max.x, rect.max.y));
        
        let Some(p1) = p1 else { return };
        let Some(p2) = p2 else { return };
        
        let x_range = crate::region::Range::new(p1.x.min(p2.x), p1.x.max(p2.x));
        let y_range = crate::region::Range::new(p1.y.min(p2.y), p1.y.max(p2.y));
        
        for region in regions {
            if !region.visible {
                continue;
            }
            
            if let Some(tr) = region.time_range() {
                if tr.intersects(&x_range) {
                    if let Some(vr) = region.value_range() {
                        if vr.intersects(&y_range) {
                            self.regions.insert(region.id);
                        }
                    } else {
                        self.regions.insert(region.id);
                    }
                }
            }
        }
    }
    
    /// Select all points in a series within a data rectangle
    pub fn select_points_in_rect(
        &mut self,
        series: &Series,
        x_range: crate::region::Range,
        y_range: crate::region::Range,
    ) {
        let set = self.points.entry(series.id()).or_default();
        for (i, point) in series.points().iter().enumerate() {
            if x_range.contains(point.x) && y_range.contains(point.y) {
                set.insert(i);
            }
        }
    }
}

// Helper function
fn rect_contains(rect: crate::geom::ScreenRect, point: crate::geom::ScreenPoint) -> bool {
    point.x >= rect.min.x && point.x <= rect.max.x &&
    point.y >= rect.min.y && point.y <= rect.max.y
}

#[cfg(test)]
fn iced_liveplot_next_series_id() -> crate::series::SeriesId {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    crate::series::SeriesId::from_raw(900_000 + id)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_selection_basic() {
        let mut sel = Selection::new();
        assert!(sel.is_empty());
        
        let pin = Pin {
            series: iced_liveplot_next_series_id(),
            index: 0,
        };
        
        sel.select_pin(pin);
        assert!(!sel.is_empty());
        assert_eq!(sel.len(), 1);
        assert!(sel.is_pin_selected(&pin));
        
        sel.deselect_pin(&pin);
        assert!(sel.is_empty());
    }
    
    #[test]
    fn test_region_selection() {
        let mut sel = Selection::new();
        let id = RegionId::new();
        
        sel.select_region(id);
        assert!(sel.is_region_selected(&id));
        
        sel.toggle_region(id);
        assert!(!sel.is_region_selected(&id));
        
        sel.toggle_region(id);
        assert!(sel.is_region_selected(&id));
    }
    
    #[test]
    fn test_point_selection() {
        let mut sel = Selection::new();
        let series = iced_liveplot_next_series_id();
        
        sel.select_point(series, 5);
        assert!(sel.is_point_selected(series, 5));
        assert!(!sel.is_point_selected(series, 6));
        
        sel.select_point(series, 10);
        assert!(sel.is_point_selected(series, 10));
        
        let points = sel.selected_points(series).unwrap();
        assert_eq!(points.len(), 2);
        assert!(points.contains(&5));
        assert!(points.contains(&10));
    }
}