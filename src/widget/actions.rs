//! Action system for the plot widget.
//!
//! This module provides a command pattern implementation for user interactions
//! that can be undone/redone. Actions are executed through the `ActionDispatcher`.

use crate::datasource::{AppendError, AppendOnlyData};
use crate::geom::Point;
use crate::interaction::Pin;
use crate::plot::Plot;
use crate::series::SeriesId;
use crate::transform::Transform;
use crate::view::{Range, Viewport};
use crate::widget::state::{ PinToggle, PlotUiState};
use crate::widget::config::PlotViewConfig;
use std::collections::HashMap;
use iced::time::Instant;
use std::sync::Arc;

/// Error type for action execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionError {
    /// The action cannot be undone.
    NotReversible,
    /// Invalid parameters for the action.
    InvalidParameters(&'static str),
    /// Append operation failed.
    AppendFailed(AppendError),
    /// General action failure.
    Failed(&'static str),
}

impl From<AppendError> for ActionError {
    fn from(e: AppendError) -> Self {
        ActionError::AppendFailed(e)
    }
}

/// Result of an action execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionResult {
    /// Action completed successfully, redraw needed.
    Redraw,
    /// Action completed but no visual change.
    None,
    /// Action failed.
    Error(ActionError),
}

/// Context provided to actions during execution.
pub struct ActionContext<'a> {
    /// The plot being modified.
    pub plot: &'a mut Plot,
    /// The UI state for pins, hover, etc. (optional: widget event state may
    /// not be available outside the canvas draw/event cycle).
    pub state: Option<&'a mut PlotUiState>,
    /// The clipboard for copy/paste.
    pub clipboard: &'a mut Clipboard,
    /// The history group for batching actions.
    pub history: Option<&'a mut HistoryGroup>,
    /// Plot view configuration.
    pub config: &'a PlotViewConfig,
}

/// Trait for plot actions.
pub trait PlotAction: Send + Sync + 'static {
    /// Human-readable name of the action.
    fn name(&self) -> &'static str;
    
    /// Execute the action.
    fn execute(&self, ctx: &mut ActionContext<'_>) -> ActionResult;
    
    /// Whether this action can be undone.
    fn can_undo(&self) -> bool {
        false
    }
    
    /// Undo the action. Called only if `can_undo` returns true.
    fn undo(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        ActionResult::Error(ActionError::NotReversible)
    }
}

/// Clipboard for copy/paste operations.
#[derive(Debug, Clone, Default)]
pub struct Clipboard {
    pub points: Vec<Point>,
    pub series: Option<SeriesId>,
    pub regions: Vec<crate::region::DataRegion>,
}

/// Group of actions that can be undone/redone together.
#[derive(Clone)]
pub struct HistoryGroup {
    pub actions: Vec<Arc<dyn PlotAction + 'static>>,
    pub description: String,
    pub timestamp: Instant,
}

impl HistoryGroup {
    pub fn new(description: String) -> Self {
        Self {
            actions: Vec::new(),
            description,
            timestamp: Instant::now(),
        }
    }
    
    pub fn add_action(&mut self, action: Arc<dyn PlotAction + 'static>) {
        self.actions.push(action);
    }
    
    pub fn description(&self) -> &str {
        &self.description
    }
    
    pub fn actions(&self) -> &[Arc<dyn PlotAction + 'static>] {
        &self.actions
    }
    
    pub fn action_count(&self) -> usize {
        self.actions.len()
    }
}

/// Copy the given regions into the clipboard (no plot mutation).
#[derive(Debug, Clone)]
pub struct CopyRegions {
    pub ids: Vec<crate::region::RegionId>,
}

impl PlotAction for CopyRegions {
    fn name(&self) -> &'static str {
        "CopyRegions"
    }

    fn execute(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        ctx.clipboard.regions.clear();
        for id in &self.ids {
            if let Some(region) = ctx.plot.regions().iter().find(|r| r.id == *id) {
                ctx.clipboard.regions.push(region.clone());
            }
        }
        ActionResult::None
    }

    fn can_undo(&self) -> bool {
        false
    }
}

/// Paste the clipboard contents after the last clipboard region, with a
/// step offset (in data units) so pastes don't stack exactly on top.
#[derive(Debug, Clone)]
pub struct PasteRegions {
    pub offset: f64,
}

impl PasteRegions {
    pub fn new(offset: f64) -> Self {
        Self { offset }
    }
}

impl PlotAction for PasteRegions {
    fn name(&self) -> &'static str {
        "PasteRegions"
    }

    fn execute(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        if ctx.clipboard.regions.is_empty() {
            return ActionResult::None;
        }
        // Offset pasted copies by a bar (or 4 beats worth) so they don't overlap
        let step = if self.offset == 0.0 { 4.0 * 60.0 / 120.0 } else { self.offset };
        let start = ctx
            .clipboard
            .regions
            .iter()
            .filter_map(|r| r.time_range().map(|t| t.max))
            .fold(0.0, f64::max);
        let pasted: Vec<crate::region::DataRegion> = ctx
            .clipboard
            .regions
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .map(|mut region| {
                let delta = start + step;
                match &mut region.kind {
                    crate::region::RegionKind::MidiNote { start, end, .. } => {
                        *start += delta;
                        *end += delta;
                    }
                    crate::region::RegionKind::AudioRegion { start, .. } => {
                        *start += delta;
                    }
                    crate::region::RegionKind::AutomationPoint { time, .. } => {
                        *time += delta;
                    }
                    crate::region::RegionKind::Marker { time, .. } => {
                        *time += delta;
                    }
                    crate::region::RegionKind::Loop { start, end } => {
                        *start += delta;
                        *end += delta;
                    }
                    _ => {}
                }
                region
            })
            .collect();
        let added = std::sync::Arc::new(AddRegions { regions: pasted });
        added.execute(ctx)
    }

    fn can_undo(&self) -> bool {
        true
    }

    fn undo(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        ActionResult::Error(ActionError::NotReversible)
    }
}

/// Action to add a pin at a specific index in a series.
#[derive(Debug)]
pub struct AddPin {
    pub series: SeriesId,
    pub index: usize,
}

impl PlotAction for AddPin {
    fn name(&self) -> &'static str {
        "AddPin"
    }
    
    fn execute(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        let series = match ctx.plot.series_mut().iter_mut().find(|s| s.id() == self.series) {
            Some(s) => s,
            None => return ActionResult::Error(ActionError::InvalidParameters("Series not found")),
        };
        
        let point = series.points().get(self.index).copied();
        if point.is_some() {
            let pin = Pin {
                series: self.series,
                index: self.index,
            };
            ctx.plot.pins_mut().push(pin);
            ActionResult::Redraw
        } else {
            ActionResult::Error(ActionError::InvalidParameters("Index out of bounds"))
        }
    }
    
    fn can_undo(&self) -> bool {
        true
    }
    
    fn undo(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        ctx.plot.pins_mut().retain(|p| !(p.series == self.series && p.index == self.index));
        ActionResult::Redraw
    }
}

/// Action to remove a pin.
#[derive(Debug)]
pub struct RemovePin {
    pub pin: Pin,
}

impl PlotAction for RemovePin {
    fn name(&self) -> &'static str {
        "RemovePin"
    }
    
    fn execute(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        ctx.plot.pins_mut().retain(|p| *p != self.pin);
        ActionResult::Redraw
    }
    
    fn can_undo(&self) -> bool {
        true
    }
    
    fn undo(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        ctx.plot.pins_mut().push(self.pin);
        ActionResult::Redraw
    }
}

/// Action to toggle a pin.
#[derive(Debug)]
pub struct TogglePin {
    pub pin: Pin,
}

impl PlotAction for TogglePin {
    fn name(&self) -> &'static str {
        "TogglePin"
    }
    
    fn execute(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        crate::interaction::toggle_pin(ctx.plot.pins_mut(), self.pin);
        ActionResult::Redraw
    }
    
    fn can_undo(&self) -> bool {
        true
    }
    
    fn undo(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        crate::interaction::toggle_pin(ctx.plot.pins_mut(), self.pin);
        ActionResult::Redraw
    }
}

/// Action to add a data point to a series.
#[derive(Debug)]
pub struct AddPoint {
    pub series: SeriesId,
    pub point: Point,
}

impl PlotAction for AddPoint {
    fn name(&self) -> &'static str {
        "AddPoint"
    }
    
    fn execute(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        let series = match ctx.plot.series_mut().iter_mut().find(|s| s.id() == self.series) {
            Some(s) => s,
            None => return ActionResult::Error(ActionError::InvalidParameters("Series not found")),
        };
        
        // Add point to series
        let mut series_mut = series.clone();
        series_mut.push_y(self.point.y).ok();
        // Would need to replace series in plot
        ActionResult::Redraw
    }
    
    fn can_undo(&self) -> bool {
        true
    }
    
    fn undo(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        // Would remove the last point
        ActionResult::Error(ActionError::NotReversible)
    }
}

/// Action to remove data points from a series.
#[derive(Debug)]
pub struct RemovePoints {
    pub series: SeriesId,
    pub range: std::ops::Range<usize>,
    deleted: Vec<Point>, // Store for undo
}

impl RemovePoints {
    pub fn new(series: SeriesId, range: std::ops::Range<usize>) -> Self {
        Self {
            series,
            range,
            deleted: Vec::new(),
        }
    }
}

impl PlotAction for RemovePoints {
    fn name(&self) -> &'static str {
        "RemovePoints"
    }
    
    fn execute(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        let series = match ctx.plot.series_mut().iter_mut().find(|s| s.id() == self.series) {
            Some(s) => s,
            None => return ActionResult::Error(ActionError::InvalidParameters("Series not found")),
        };
        
        let points = series.points();
        if self.range.end > points.len() {
            return ActionResult::Error(ActionError::InvalidParameters("Range out of bounds"));
        }
        
        // Store deleted points for undo
        let deleted: Vec<Point> = points[self.range.clone()].to_vec();
        
        series.remove_points(self.range.start, self.range.len());
        ActionResult::Redraw
    }
    
    fn can_undo(&self) -> bool {
        true
    }
    
    fn undo(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        // Would need to reinsert deleted points
        ActionResult::Error(ActionError::NotReversible)
    }
}

/// Action to insert data points into a series.
#[derive(Debug)]
pub struct InsertPoints {
    pub series: SeriesId,
    pub index: usize,
    pub points: Vec<Point>,
}

impl PlotAction for InsertPoints {
    fn name(&self) -> &'static str {
        "InsertPoints"
    }
    
    fn execute(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        let series = match ctx.plot.series_mut().iter_mut().find(|s| s.id() == self.series) {
            Some(s) => s,
            None => return ActionResult::Error(ActionError::InvalidParameters("Series not found")),
        };
        
        series.insert_points(self.index, &self.points);
        ActionResult::Redraw
    }
    
    fn can_undo(&self) -> bool {
        true
    }
    
    fn undo(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        let series = match ctx.plot.series_mut().iter_mut().find(|s| s.id() == self.series) {
            Some(s) => s,
            None => return ActionResult::Error(ActionError::InvalidParameters("Series not found")),
        };
        
        series.remove_points(self.index, self.points.len());
        ActionResult::Redraw
    }
}

/// Action to set a data point in a series.
#[derive(Debug)]
pub struct SetPoint {
    pub series: SeriesId,
    pub index: usize,
    pub point: Point,
    old: Option<Point>,
}

impl PlotAction for SetPoint {
    fn name(&self) -> &'static str {
        "SetPoint"
    }
    
    fn execute(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        let series = match ctx.plot.series_mut().iter_mut().find(|s| s.id() == self.series) {
            Some(s) => s,
            None => return ActionResult::Error(ActionError::InvalidParameters("Series not found")),
        };
        
        let points = series.points();
        if self.index >= points.len() {
            return ActionResult::Error(ActionError::InvalidParameters("Index out of bounds"));
        }
        
        let old = Some(points[self.index]);
        series.set_point(self.index, self.point);
        ActionResult::Redraw
    }
    
    fn can_undo(&self) -> bool {
        self.old.is_some()
    }
    
    fn undo(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        if let Some(old) = self.old {
            let series = match ctx.plot.series_mut().iter_mut().find(|s| s.id() == self.series) {
                Some(s) => s,
                None => return ActionResult::Error(ActionError::InvalidParameters("Series not found")),
            };
            series.set_point(self.index, old);
            ActionResult::Redraw
        } else {
            ActionResult::Error(ActionError::NotReversible)
        }
    }
}

/// Action adding regions to the plot (insert note/clip/marker/pen point).
#[derive(Debug, Clone)]
pub struct AddRegions {
    pub regions: Vec<crate::region::DataRegion>,
}

impl PlotAction for AddRegions {
    fn name(&self) -> &'static str {
        "AddRegions"
    }

    fn execute(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        for region in &self.regions {
            ctx.plot.add_region(region.clone());
        }
        ActionResult::Redraw
    }

    fn can_undo(&self) -> bool {
        true
    }

    fn undo(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        for region in &self.regions {
            ctx.plot.remove_region(region.id);
        }
        ActionResult::Redraw
    }
}

/// Action removing regions from the plot, storing them for undo.
#[derive(Debug, Clone)]
pub struct RemoveRegions {
    pub removed: Vec<crate::region::DataRegion>,
}

impl RemoveRegions {
    /// Build the action from the regions currently in the plot matching `ids`.
    pub fn capture(plot: &Plot, ids: &[crate::region::RegionId]) -> Self {
        let mut removed = Vec::new();
        for id in ids {
            if let Some(region) = plot.regions().iter().find(|r| r.id == *id) {
                removed.push(region.clone());
            }
        }
        Self { removed }
    }
}

impl PlotAction for RemoveRegions {
    fn name(&self) -> &'static str {
        "RemoveRegions"
    }

    fn execute(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        for region in &self.removed {
            ctx.plot.remove_region(region.id);
        }
        ActionResult::Redraw
    }

    fn can_undo(&self) -> bool {
        true
    }

    fn undo(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        for region in &self.removed {
            ctx.plot.add_region(region.clone());
        }
        ActionResult::Redraw
    }
}

/// Action restoring a region to a previous snapshot (undo of move/resize).
#[derive(Debug, Clone)]
pub struct RestoreRegions {
    pub before: Vec<crate::region::DataRegion>,
}

impl PlotAction for RestoreRegions {
    fn name(&self) -> &'static str {
        "RestoreRegions"
    }

    fn execute(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        for before in &self.before {
            if let Some(region) = ctx
                .plot
                .regions_mut()
                .iter_mut()
                .find(|r| r.id == before.id)
            {
                *region = before.clone();
            }
        }
        ActionResult::Redraw
    }

    fn can_undo(&self) -> bool {
        false
    }
}

/// Action to set the viewport.
#[derive(Debug)]
pub struct SetViewport {
    pub viewport: Viewport,
}

impl PlotAction for SetViewport {
    fn name(&self) -> &'static str {
        "SetViewport"
    }
    
    fn execute(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        ctx.plot.set_manual_view(self.viewport);
        ActionResult::Redraw
    }
    
    fn can_undo(&self) -> bool {
        true
    }
    
    fn undo(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        // Would need to store previous viewport
        ActionResult::Error(ActionError::NotReversible)
    }
}

/// Reset view to auto
#[derive(Debug, Clone, Default)]
pub struct ResetView;

impl PlotAction for ResetView {
    fn name(&self) -> &'static str {
        "ResetView"
    }
    
    fn execute(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        ctx.plot.reset_view();
        ActionResult::Redraw
    }
    
    fn can_undo(&self) -> bool {
        false
    }
}

/// Action to toggle series visibility.
#[derive(Debug)]
pub struct ToggleSeriesVisibility {
    pub series: SeriesId,
}

impl PlotAction for ToggleSeriesVisibility {
    fn name(&self) -> &'static str {
        "ToggleSeriesVisibility"
    }
    
    fn execute(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        let series = match ctx.plot.series_mut().iter_mut().find(|s| s.id() == self.series) {
            Some(s) => s,
            None => return ActionResult::Error(ActionError::InvalidParameters("Series not found")),
        };
        
        let visible = series.is_visible();
        series.set_visible(!visible);
        ActionResult::Redraw
    }
    
    fn can_undo(&self) -> bool {
        true
    }
    
    fn undo(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        // Same operation toggles back
        self.execute(ctx)
    }
}

/// Action to change the plot theme.
#[derive(Debug)]
pub struct ChangeTheme {
    pub theme: crate::style::Theme,
}

impl PlotAction for ChangeTheme {
    fn name(&self) -> &'static str {
        "ChangeTheme"
    }
    
    fn execute(&self, ctx: &mut ActionContext<'_>) -> ActionResult {
        // Would apply theme
        ActionResult::Redraw
    }
    
    fn can_undo(&self) -> bool {
        false
    }
}

/// Action Dispatcher - handles action execution with history management
#[derive(Default)]
pub struct ActionDispatcher {
    history: Vec<HistoryGroup>,
    redo_stack: Vec<HistoryGroup>,
    max_history: usize,
    group_window: std::time::Duration,
    current_group: Option<HistoryGroup>,
    group_start: Option<Instant>,
}

impl ActionDispatcher {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            redo_stack: Vec::new(),
            max_history: 100,
            group_window: std::time::Duration::from_millis(500),
            current_group: None,
            group_start: None,
        }
    }

    pub fn with_max_history(mut self, max: usize) -> Self {
        self.max_history = max;
        self
    }

    pub fn with_group_window(mut self, duration: std::time::Duration) -> Self {
        self.group_window = duration;
        self
    }

    /// Execute a pre-erased action (dyn-compatible entry point).
    pub fn execute_dyn(&mut self, action: Arc<dyn PlotAction>, ctx: &mut ActionContext<'_>) -> ActionResult {
        let now = Instant::now();
        let can_group = if let Some(start) = self.group_start {
            now.duration_since(start) <= self.group_window
        } else {
            false
        };

        if action.can_undo() {
            if can_group && self.current_group.is_some() {
                if let Some(ref mut group) = self.current_group {
                    group.add_action(action.clone());
                }
            } else {
                let mut group = HistoryGroup::new(action.name().to_string());
                group.add_action(action.clone());
                self.current_group = Some(group);
                self.group_start = Some(now);
            }
        }

        let result = action.execute(ctx);

        if action.can_undo() {
            if let Some(group) = self.current_group.take() {
                self.history.push(group);
                if self.history.len() > self.max_history {
                    self.history.remove(0);
                }
                self.redo_stack.clear();
                self.group_start = None;
            }
        }

        result
    }

    pub fn execute<A: PlotAction + 'static>(&mut self, action: Arc<A>, ctx: &mut ActionContext<'_>) -> ActionResult {
        // Check if we should start a new group or add to current
        let now = Instant::now();
        let can_group = if let Some(start) = self.group_start {
            now.duration_since(start) <= self.group_window
        } else {
            false
        };

        let action_dyn: Arc<dyn PlotAction> = action.clone();

        if action_dyn.can_undo() {
            if can_group && self.current_group.is_some() {
                // Add to current group
                if let Some(ref mut group) = self.current_group {
                    group.add_action(action_dyn.clone());
                }
            } else {
                // Start new group
                let mut group = HistoryGroup::new(action_dyn.name().to_string());
                group.add_action(action_dyn.clone());
                self.current_group = Some(group);
                self.group_start = Some(now);
            }
        }

        let result = action_dyn.execute(ctx);

        // If action was grouped, finalize the group
        if action_dyn.can_undo() {
            if let Some(group) = self.current_group.take() {
                self.history.push(group);
                if self.history.len() > self.max_history {
                    self.history.remove(0);
                }
                self.redo_stack.clear();
                self.group_start = None;
            }
        }

        result
    }

    pub fn undo(&mut self, ctx: &mut ActionContext<'_>) -> Option<ActionResult> {
        let group = self.history.pop()?;
        let mut results = Vec::new();
        
        for action in group.actions().iter().rev() {
            let result = action.undo(ctx);
            results.push(result);
        }
        
        self.redo_stack.push(group);
        
        // Return the last result
        results.into_iter().last()
    }

    pub fn redo(&mut self, ctx: &mut ActionContext<'_>) -> Option<ActionResult> {
        let group = self.redo_stack.pop()?;
        let mut results = Vec::new();
        
        for action in group.actions() {
            let result = action.execute(ctx);
            results.push(result);
        }
        
        self.history.push(group);
        
        results.into_iter().last()
    }

    pub fn can_undo(&self) -> bool {
        !self.history.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn history(&self) -> &[HistoryGroup] {
        &self.history
    }

    pub fn redo_stack(&self) -> &[HistoryGroup] {
        &self.redo_stack
    }

    pub fn clear(&mut self) {
        self.history.clear();
        self.redo_stack.clear();
        self.current_group = None;
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use crate::plot::Plot;
    use crate::series::{Series, SeriesId};
    use crate::widget::state::PlotUiState;
    use crate::widget::config::PlotViewConfig;
    use std::sync::Arc;
    
    #[test]
    fn test_add_pin() {
        let mut plot = Plot::new();
        let series = crate::series::Series::scatter("test");
        let id = series.id();
        plot.add_series(&series);
        
        let mut state = PlotUiState::default();
        let mut clipboard = Clipboard::default();
        let config = PlotViewConfig::default();
        
        let _ = &mut state;
        let mut ctx = ActionContext {
            plot: &mut plot,
            state: None,
            clipboard: &mut clipboard,
            history: None,
            config: &config,
        };
        
        let action = AddPin { series: id, index: 0 };
        let result = action.execute(&mut ctx);
        assert_eq!(result, ActionResult::Redraw);
        
        // Test undo
        let undo_result = action.undo(&mut ctx);
        assert_eq!(undo_result, ActionResult::Redraw);
    }
    
    #[test]
    fn test_toggle_pin() {
        let mut plot = Plot::new();
        let series = crate::series::Series::scatter("test");
        let id = series.id();
        plot.add_series(&series);
        
        let mut state = PlotUiState::default();
        let mut clipboard = Clipboard::default();
        let config = PlotViewConfig::default();
        
        let _ = &mut state;
        let mut ctx = ActionContext {
            plot: &mut plot,
            state: None,
            clipboard: &mut clipboard,
            history: None,
            config: &config,
        };
        
        let pin = Pin { series: id, index: 0 };
        let action = TogglePin { pin };
        
        let result = action.execute(&mut ctx);
        assert_eq!(result, ActionResult::Redraw);
        
        let result = action.execute(&mut ctx);
        assert_eq!(result, ActionResult::Redraw);
    }
}
