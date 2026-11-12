//! Tool System for iced-liveplot
//!
//! Provides mode-based editing tools (Select, Pen, Eraser, Move, etc.)

use crate::geom::Point;
use crate::region::{DataRegion, RegionId};
use crate::selection::Selection;
use crate::snap::{SnapConfig, SnapEngine, SnapResult};
use crate::series::SeriesId;
use crate::transform::Transform;
use crate::widget::actions::Clipboard;
use iced::mouse::Cursor;
use std::collections::HashMap;

/// Available editing tools
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    /// Grab tool (DEFAULT) — navigation mode: left-drag on empty plot pans,
    /// left-click on data inserts a pin, double-click resets the view,
    /// right-drag box-zooms. Matches the classic plot navigation behavior.
    Grab,
    /// Select tool - click to select, drag to box-select
    Select,
    /// Pen tool - click to add points/regions
    Pen,
    /// Eraser tool - click to delete points/regions
    Eraser,
    /// Move tool - drag to move selection
    Move,
    /// Scale tool - drag to scale selection
    Scale,
    /// Stretch tool - drag to time-stretch
    Stretch,
    /// Split tool - click to split region at cursor
    Split,
    /// Join tool - drag to join adjacent regions
    Join,
    /// Draw tool - freehand draw automation
    Draw,
    /// Line tool - click-drag for straight line automation
    Line,
    /// Curve tool - click-drag for bezier curve automation
    Curve,
    /// Velocity tool - edit MIDI note velocities
    Velocity,
    /// Quantize tool - snap to grid
    Quantize,
}

impl Tool {
    /// Get all available tools
    pub fn all() -> &'static [Tool] {
        &[
            Tool::Grab,
            Tool::Select,
            Tool::Pen,
            Tool::Eraser,
            Tool::Move,
            Tool::Scale,
            Tool::Stretch,
            Tool::Split,
            Tool::Join,
            Tool::Draw,
            Tool::Line,
            Tool::Curve,
            Tool::Velocity,
            Tool::Quantize,
        ]
    }
    
    /// Get tool name for UI
    pub fn name(&self) -> &'static str {
        match self {
            Tool::Grab => "Grab",
            Tool::Select => "Select",
            Tool::Pen => "Pen",
            Tool::Eraser => "Eraser",
            Tool::Move => "Move",
            Tool::Scale => "Scale",
            Tool::Stretch => "Stretch",
            Tool::Split => "Split",
            Tool::Join => "Join",
            Tool::Draw => "Draw",
            Tool::Line => "Line",
            Tool::Curve => "Curve",
            Tool::Velocity => "Velocity",
            Tool::Quantize => "Quantize",
        }
    }
    
    /// Get tool description
    pub fn description(&self) -> &'static str {
        match self {
            Tool::Grab => "Pan (drag), pin (click data), zoom box (right-drag), double-click reset",
            Tool::Select => "Click to select, drag to box-select",
            Tool::Pen => "Click to add points/regions",
            Tool::Eraser => "Click to delete points/regions",
            Tool::Move => "Drag to move selection",
            Tool::Scale => "Drag to scale selection",
            Tool::Stretch => "Drag to time-stretch",
            Tool::Split => "Click to split region",
            Tool::Join => "Drag to join regions",
            Tool::Draw => "Freehand draw automation",
            Tool::Line => "Straight line automation",
            Tool::Curve => "Bezier curve automation",
            Tool::Velocity => "Edit note velocities",
            Tool::Quantize => "Snap to grid",
        }
    }
    
    /// Get default cursor for tool
    pub fn cursor(&self) -> iced::mouse::Interaction {
        match self {
            Tool::Grab => iced::mouse::Interaction::Grabbing,
            Tool::Select => iced::mouse::Interaction::Crosshair,
            Tool::Pen => iced::mouse::Interaction::Crosshair,
            Tool::Eraser => iced::mouse::Interaction::Crosshair,
            Tool::Move => iced::mouse::Interaction::Move,
            Tool::Scale => iced::mouse::Interaction::ResizingHorizontally,
            Tool::Stretch => iced::mouse::Interaction::ResizingHorizontally,
            Tool::Split => iced::mouse::Interaction::Crosshair,
            Tool::Join => iced::mouse::Interaction::Crosshair,
            Tool::Draw => iced::mouse::Interaction::Crosshair,
            Tool::Line => iced::mouse::Interaction::Crosshair,
            Tool::Curve => iced::mouse::Interaction::Crosshair,
            Tool::Velocity => iced::mouse::Interaction::Crosshair,
            Tool::Quantize => iced::mouse::Interaction::Crosshair,
        }
    }
}

/// Tool context passed to tool handlers
pub struct ToolContext<'a> {
    pub tool: Tool,
    pub modifiers: iced::keyboard::Modifiers,
    pub snap: &'a mut SnapEngine,
    pub selection: &'a mut Selection,
    pub clipboard: &'a mut Clipboard,
    pub transform: &'a Transform,
    pub view_range: crate::region::Range,
    pub bpm: f64,
    pub tool_state: &'a mut ToolState,
}

/// Tool gesture state
#[derive(Debug, Clone)]
pub enum ToolGesture {
    None,
    /// Drag gesture for box select, move, scale, etc.
    Drag {
        start: Point,
        current: Point,
        start_screen: crate::geom::ScreenPoint,
        current_screen: crate::geom::ScreenPoint,
    },
    /// Click gesture for pen, eraser, split
    Click {
        position: Point,
        screen: crate::geom::ScreenPoint,
    },
    /// Drawing gesture for freehand
    Draw {
        points: Vec<Point>,
    },
}

impl ToolGesture {
    pub fn is_active(&self) -> bool {
        !matches!(self, ToolGesture::None)
    }
}

/// The in-progress gesture of the Move / Scale / Stretch tools.
///
/// DAW piano-roll semantics:
/// * a drag **only** starts when the press lands on a region (event);
/// * pressing empty space does nothing (no phantom move);
/// * the grabbed offset is remembered per region so the whole selection
///   moves *with* the cursor, regardless of where inside the event the user
///   pressed;
/// * grabbing within a few pixels of an edge switches to resizing that edge
///   (Scale = start, Stretch = end).
#[derive(Debug, Clone)]
pub struct MoveGesture {
    /// The region that was pressed (and whose edges were tested).
    pub primary: Option<crate::region::RegionId>,
    /// Data-space point of the press.
    pub grab: crate::geom::Point,
    /// Data-space point of the current drag position.
    pub current: crate::geom::Point,
    /// Zone grabbed (`None` = middle → move).
    pub zone: Option<TrimZone>,
    /// All regions participating in the gesture (primary + box-selection).
    /// Per-region grab offsets are stored alongside so every event keeps its
    /// relative distance to the cursor.
    pub targets: Vec<(crate::region::RegionId, f64, f64)>,
    /// True while the gesture is live.
    pub active: bool,
}

impl Default for MoveGesture {
    fn default() -> Self {
        Self {
            primary: None,
            grab: crate::geom::Point::new(0.0, 0.0),
            current: crate::geom::Point::new(0.0, 0.0),
            zone: None,
            targets: Vec::new(),
            active: false,
        }
    }
}

impl MoveGesture {
    fn clear(&mut self) {
        self.active = false;
        self.primary = None;
        self.zone = None;
        self.targets.clear();
    }

    /// Screen-space X travel of the current gesture, in data units.
    pub fn dx(&self) -> f64 {
        self.current.x - self.grab.x
    }

    /// Screen-space Y travel of the current gesture, in data units.
    pub fn dy(&self) -> f64 {
        self.current.y - self.grab.y
    }
}

/// Width (in screen pixels) of the start/end trim zones.
///
/// Fixed PIXEL width (not data units) so the grab areas feel identical at any
/// zoom level — exactly how Logic Pro / Ableton behave.
pub const TRIM_ZONE_PX: f32 = 9.0;

/// Which part of an event the pointer is over.
///
/// This is the single source of truth for the tool mechanism:
/// * [`TrimZone::Start`] / [`TrimZone::End`] → dragging trims that edge;
/// * [`TrimZone::Move`] → dragging moves the whole event/selection;
/// * [`TrimZone::None`] → nothing under the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrimZone {
    Start,
    End,
    Move,
}

/// Tool state machine
pub struct ToolState {
    pub current_tool: Tool,
    pub gesture: ToolGesture,
    pub last_cursor: Option<crate::geom::ScreenPoint>,
    /// Live Move/Scale/Stretch gesture (see [`MoveGesture`]).
    pub drag_move: MoveGesture,
    /// Regions flagged selected in the plot model (box-select result).
    /// Kept as a cache for the widget's own checks; the plot model is the
    /// authority and is re-read when a gesture starts.
    pub selected_regions: Vec<crate::region::RegionId>,
}

impl Default for ToolState {
    fn default() -> Self {
        Self {
            current_tool: Tool::Grab,
            gesture: ToolGesture::None,
            last_cursor: None,
            drag_move: MoveGesture::default(),
            selected_regions: Vec::new(),
        }
    }
}

impl ToolState {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn set_tool(&mut self, tool: Tool) {
        self.current_tool = tool;
        self.gesture = ToolGesture::None;
    }
    
    pub fn current_tool(&self) -> Tool {
        self.current_tool
    }
    
    pub fn gesture(&self) -> &ToolGesture {
        &self.gesture
    }
    
    pub fn gesture_mut(&mut self) -> &mut ToolGesture {
        &mut self.gesture
    }
    
    pub fn start_drag(&mut self, start: Point, start_screen: crate::geom::ScreenPoint) {
        self.gesture = ToolGesture::Drag {
            start,
            current: start,
            start_screen,
            current_screen: start_screen,
        };
    }
    
    pub fn update_drag(&mut self, current: Point, current_screen: crate::geom::ScreenPoint) {
        if let ToolGesture::Drag { current: cur, current_screen: cur_screen, .. } = &mut self.gesture {
            *cur = current;
            *cur_screen = current_screen;
        }
    }
    
    pub fn end_drag(&mut self) -> Option<ToolGesture> {
        let gesture = std::mem::replace(&mut self.gesture, ToolGesture::None);
        if gesture.is_active() {
            Some(gesture)
        } else {
            None
        }
    }
    
    pub fn start_click(&mut self, position: Point, screen: crate::geom::ScreenPoint) {
        self.gesture = ToolGesture::Click { position, screen };
    }
    
    pub fn start_draw(&mut self, position: Point) {
        self.gesture = ToolGesture::Draw { points: vec![position] };
    }
    
    pub fn add_draw_point(&mut self, position: Point) {
        if let ToolGesture::Draw { points } = &mut self.gesture {
            points.push(position);
        }
    }
    
    pub fn end_draw(&mut self) -> Option<Vec<Point>> {
        if let ToolGesture::Draw { points } = std::mem::replace(&mut self.gesture, ToolGesture::None) {
            Some(points)
        } else {
            None
        }
    }
}

/// Result of a tool interaction
#[derive()]
pub enum ToolResult {
    /// No action needed
    None,
    /// Request a redraw
    Redraw,
    /// An action was created (for undo/redo)
    Action(std::sync::Arc<dyn crate::widget::actions::PlotAction>),
    /// Multiple actions (for grouped operations)
    Actions(Vec<std::sync::Arc<dyn crate::widget::actions::PlotAction>>),
    /// Custom message
    Custom(String),
}

/// Tool handler trait - implement for custom tools
pub trait ToolHandler: Send + Sync {
    fn handle_mouse_down(&mut self, ctx: &mut ToolContext, pos: Point, screen: crate::geom::ScreenPoint) -> ToolResult;
    fn handle_mouse_move(&mut self, ctx: &mut ToolContext, pos: Point, screen: crate::geom::ScreenPoint) -> ToolResult;
    fn handle_mouse_up(&mut self, ctx: &mut ToolContext, pos: Point, screen: crate::geom::ScreenPoint) -> ToolResult;
    fn handle_key_down(&mut self, ctx: &mut ToolContext, key: iced::keyboard::Key) -> ToolResult;
    fn handle_key_up(&mut self, ctx: &mut ToolContext, key: iced::keyboard::Key) -> ToolResult;
    fn cursor(&self) -> iced::mouse::Interaction;
}

/// Default tool handler implementation
pub struct DefaultToolHandler;

impl ToolHandler for DefaultToolHandler {
    fn handle_mouse_down(&mut self, ctx: &mut ToolContext, pos: Point, screen: crate::geom::ScreenPoint) -> ToolResult {
        match ctx.tool {
            Tool::Select => {
                ctx.tool_state.start_drag(pos, screen);
                ToolResult::None
            }
            Tool::Pen => {
                ctx.tool_state.start_click(pos, screen);
                ToolResult::None
            }
            Tool::Eraser => {
                ctx.tool_state.start_click(pos, screen);
                ToolResult::None
            }
            Tool::Move => {
                if !ctx.selection.is_empty() {
                    ctx.tool_state.start_drag(pos, screen);
                }
                ToolResult::None
            }
            Tool::Split => {
                ctx.tool_state.start_click(pos, screen);
                ToolResult::None
            }
            Tool::Draw => {
                ctx.tool_state.start_draw(pos);
                ToolResult::None
            }
            Tool::Line | Tool::Curve => {
                ctx.tool_state.start_drag(pos, screen);
                ToolResult::None
            }
            _ => ToolResult::None,
        }
    }
    
    fn handle_mouse_move(&mut self, ctx: &mut ToolContext, pos: Point, screen: crate::geom::ScreenPoint) -> ToolResult {
        match ctx.tool {
            Tool::Select | Tool::Move | Tool::Scale | Tool::Stretch | Tool::Line | Tool::Curve => {
                if ctx.tool_state.gesture().is_active() {
                    ctx.tool_state.update_drag(pos, screen);
                    ToolResult::Redraw
                } else {
                    ToolResult::None
                }
            }
            Tool::Draw => {
                if ctx.tool_state.gesture().is_active() {
                    ctx.tool_state.add_draw_point(pos);
                    ToolResult::Redraw
                } else {
                    ToolResult::None
                }
            }
            _ => ToolResult::None,
        }
    }
    
    fn handle_mouse_up(&mut self, ctx: &mut ToolContext, pos: Point, screen: crate::geom::ScreenPoint) -> ToolResult {
        match ctx.tool {
            Tool::Select => {
                if let Some(gesture) = ctx.tool_state.end_drag() {
                    if let ToolGesture::Drag { start, current, start_screen, current_screen } = gesture {
                        // Box selection
                        let rect = crate::geom::ScreenRect::new(
                            crate::geom::ScreenPoint::new(start_screen.x.min(current_screen.x), start_screen.y.min(current_screen.y)),
                            crate::geom::ScreenPoint::new(start_screen.x.max(current_screen.x), start_screen.y.max(current_screen.y)),
                        );
                        
                        // Check if it's a click (small drag)
                        let dist = ((start_screen.x - current_screen.x).powi(2) + (start_screen.y - current_screen.y).powi(2)).sqrt();
                        if dist < 5.0 {
                            // Single click - select/deselect at position
                            // This would hit-test regions/pins
                        } else {
                            // Box select
                            // Select regions in rect
                        }
                    }
                }
                ToolResult::Redraw
            }
            Tool::Pen => {
                if let Some(_gesture) = ctx.tool_state.end_drag() {
                    // Add point/region at position
                    // Snap if enabled
                    let _snapped = ctx.snap.snap(pos, &[], ctx.transform, ctx.view_range, ctx.bpm);
                    ToolResult::None // Requires an actual series context; handled at event layer
                } else {
                    ToolResult::None
                }
            }
            Tool::Eraser => {
                if let Some(_gesture) = ctx.tool_state.end_drag() {
                    // Delete at position
                    ToolResult::None
                } else {
                    ToolResult::None
                }
            }
            Tool::Move => {
                if let Some(gesture) = ctx.tool_state.end_drag() {
                    if let ToolGesture::Drag { start, current, .. } = gesture {
                        // Move selection by delta
                        let delta = Point::new(current.x - start.x, current.y - start.y);
                        // Create move actions for selected items
                    }
                }
                ToolResult::None
            }
            Tool::Split => {
                if let Some(_gesture) = ctx.tool_state.end_drag() {
                    // Split region at position
                    ToolResult::None
                } else {
                    ToolResult::None
                }
            }
            Tool::Draw => {
                if let Some(points) = ctx.tool_state.end_draw() {
                    // Create automation points from drawn path
                    ToolResult::None
                } else {
                    ToolResult::None
                }
            }
            Tool::Line | Tool::Curve => {
                if let Some(gesture) = ctx.tool_state.end_drag() {
                    if let ToolGesture::Drag { start, current, .. } = gesture {
                        // Create line/curve automation
                    }
                }
                ToolResult::None
            }
            _ => ToolResult::None,
        }
    }
    
    fn handle_key_down(&mut self, _ctx: &mut ToolContext, _key: iced::keyboard::Key) -> ToolResult {
        ToolResult::None
    }
    
    fn handle_key_up(&mut self, _ctx: &mut ToolContext, _key: iced::keyboard::Key) -> ToolResult {
        ToolResult::None
    }
    
    fn cursor(&self) -> iced::mouse::Interaction {
        iced::mouse::Interaction::Crosshair
    }
}

/// Tool registry for managing available tools
pub struct ToolRegistry<'a> {
    handlers: HashMap<Tool, Box<dyn ToolHandler + 'a>>,
}

impl<'a> Default for ToolRegistry<'a> {
    fn default() -> Self {
        let mut registry = Self {
            handlers: HashMap::new(),
        };
        
        // Register default handler for all tools
        for tool in Tool::all() {
            registry.register(*tool, Box::new(DefaultToolHandler));
        }
        
        registry
    }
}

impl<'a> ToolRegistry<'a> {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn register(&mut self, tool: Tool, handler: Box<dyn ToolHandler + 'a>) {
        self.handlers.insert(tool, handler);
    }
    
    pub fn get(&self, tool: Tool) -> Option<&dyn ToolHandler> {
        self.handlers.get(&tool).map(|h| h.as_ref())
    }
    
    pub fn get_mut(&mut self, tool: Tool) -> Option<&mut (dyn ToolHandler + 'a)> {
        self.handlers.get_mut(&tool).map(|h| h.as_mut())
    }
    
    pub fn all_tools(&self) -> Vec<Tool> {
        self.handlers.keys().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tool_registry() {
        let registry = ToolRegistry::new();
        assert!(registry.get(Tool::Select).is_some());
        assert!(registry.get(Tool::Pen).is_some());
    }
    
    #[test]
    fn test_tool_state() {
        let mut state = ToolState::new();
        assert_eq!(state.current_tool(), Tool::Select);
        
        state.set_tool(Tool::Pen);
        assert_eq!(state.current_tool(), Tool::Pen);
        
        state.start_drag(Point::new(0.0, 0.0), crate::geom::ScreenPoint::new(100.0, 100.0));
        assert!(state.gesture().is_active());
        
        state.end_drag();
        assert!(!state.gesture().is_active());
    }
}
impl TrimZone {
    /// Detect which zone a screen point falls into for a region.
    ///
    /// Uses FIXED pixel widths so the trim areas are equally easy to grab at
    /// every zoom level (DAW behaviour). Regions narrower than twice the trim
    /// width always report [`TrimZone::Move`] so tiny notes stay draggable.
    pub fn zone_for(
        screen: crate::geom::ScreenPoint,
        region: &crate::region::DataRegion,
        transform: &crate::transform::Transform,
    ) -> Option<TrimZone> {
        let bounds = region.screen_bounds(transform)?;
        // Vertical containment
        let top = bounds.min.y.min(bounds.max.y);
        let bottom = bounds.min.y.max(bounds.max.y);
        let inside_y = screen.y >= top && screen.y <= bottom;
        let x = screen.x;
        let left = bounds.min.x.min(bounds.max.x);
        let right = bounds.min.x.max(bounds.max.x);
        let inside_x = x >= left && x <= right;
        if !inside_x {
            // Allow a small horizontal tolerance so clicks just outside the
            // event's outline still grab it.
            let tol = TRIM_ZONE_PX * 0.5;
            if x < left - tol || x > right + tol {
                return None;
            }
        }
        let width = right - left;
        let zone = if width <= TRIM_ZONE_PX * 2.0 {
            // Too narrow: everything is a move.
            TrimZone::Move
        } else if x - left <= TRIM_ZONE_PX {
            TrimZone::Start
        } else if right - x <= TRIM_ZONE_PX {
            TrimZone::End
        } else {
            TrimZone::Move
        };
        // If the cursor is outside vertically the hit-test fails, except for
        // full-height regions (loop markers) which intentionally span the plot.
        Some(zone).filter(|_| inside_y || region.value_range().is_none())
    }


}
