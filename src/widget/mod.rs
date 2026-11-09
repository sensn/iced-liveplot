//! The iced backend: a [`canvas::Program`] that renders a [`Plot`] and
//! translates mouse events into plot interactions (pan, zoom, pins, hover,
//! box-select, legend toggles).

pub mod actions;
pub mod config;
pub mod constants;
pub mod dispatcher;
pub mod frame;
pub mod geometry;
pub mod history;
pub mod hover;
pub mod keybindings;
pub mod link;
pub mod paint;
pub mod state;
pub mod text;
pub mod tools;

mod events;

use std::cell::RefCell;

use iced::mouse;
use iced::widget::canvas::{self, Cache, Event, Geometry};
use iced::Rectangle;

use crate::plot::Plot;
use crate::style::Theme;
use crate::widget::actions::{ActionDispatcher, Clipboard};

pub use config::PlotViewConfig;
use state::PlotUiState;
// linking
use crate::widget::link::PlotLinkOptions;
use crate::widget::link::{PlotLinkGroup, LinkBinding};
pub use actions::PlotAction;
pub use keybindings::{KeyBindings, StandardAction};
pub use tools::{Tool, ToolRegistry, ToolState};
pub use crate::snap::SnapEngine;

/// A message produced by the plot widget.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    /// Live data tick (from a `time::every` subscription) — refresh and
    /// redraw.
    Tick,
    /// Interaction changed the view or UI state; redraw.
    Redraw,
    /// Toggle the visibility of the series named `name`.
    ToggleSeries(String),
    ResetView, // HIER HINZUFÜGEN
}

/// The plot widget program: owns the plot model and renders it on a canvas.
///
/// The plot lives behind a [`RefCell`] because `canvas::Program::draw` and
/// `update` take `&self`, while the frame builder needs `&mut Plot` to refresh
/// the viewport and hover targets.
pub struct Program {
    plot: RefCell<Plot>,
    theme: RefCell<Theme>,
    config: PlotViewConfig,
    cache: Cache,
    link_binding: Option<LinkBinding>, // HIER ERGÄNZEN: Das Binding für die Synchronisation
    /// Active editing tool state (shared with the app via accessor).
    pub(crate) tool_state: RefCell<ToolState>,
    /// Snap engine used by pen/split tools and rendering of snap guides.
    pub(crate) snap: RefCell<SnapEngine>,
    /// Optional callback invoked when a tool wants to mutate domain data
    /// (insert point, split region, ...). The app decides what to do.
    pub(crate) tool_event: RefCell<Option<ToolEventCallback>>,
    /// Undo/redo history for widget-level edits (pins, viewport, points).
    pub(crate) history: RefCell<crate::widget::actions::ActionDispatcher>,
    /// Clipboard shared by copy/paste actions.
    pub(crate) clipboard: RefCell<crate::widget::actions::Clipboard>,
}

/// Callback signature for tool-originated domain mutations.
pub type ToolEventCallback = std::rc::Rc<dyn Fn(ToolEvent) -> bool>;

/// Events emitted by tools to the host application.
#[derive(Debug, Clone)]
pub enum ToolEvent {
    /// Insert a data point at the given data-space coordinates.
    InsertPoint { series: crate::series::SeriesId, point: crate::geom::Point },
    /// Delete the point nearest to the given coordinates.
    DeletePoint { series: crate::series::SeriesId, index: usize },
    /// Delete the region under the cursor.
    DeleteRegion { region: crate::region::RegionId },
    /// Split the region under the cursor at the given time.
    SplitRegion { region: crate::region::RegionId, time: f64 },
    /// Move selection by a data-space delta.
    MoveSelection { dx: f64, dy: f64 },
    /// Log an informational message (for status display).
    Notify(String),
}

impl Program {
    pub fn new(plot: Plot) -> Self {
        Self {
            plot: RefCell::new(plot),
            theme: RefCell::new(Theme::default()),
            config: PlotViewConfig::default(),
            cache: Cache::new(),
            link_binding: None, // Initial leer
            tool_state: RefCell::new(ToolState::new()),
            snap: RefCell::new(SnapEngine::new(crate::snap::SnapConfig::default())),
            tool_event: RefCell::new(None),
            history: RefCell::new(crate::widget::actions::ActionDispatcher::new()),
            clipboard: RefCell::new(crate::widget::actions::Clipboard::default()),
        }
    }

    /// Access the tool state (current tool, active gesture).
    pub fn tool_state(&self) -> std::cell::Ref<'_, ToolState> {
        self.tool_state.borrow()
    }

    /// Mutably access the tool state.
    pub fn with_tool_state<R>(&self, f: impl FnOnce(&mut ToolState) -> R) -> R {
        f(&mut self.tool_state.borrow_mut())
    }

    /// Set the active tool.
    pub fn set_tool(&self, tool: Tool) {
        self.with_tool_state(|ts| ts.set_tool(tool));
        self.request_redraw();
    }

    /// The active tool.
    pub fn current_tool(&self) -> Tool {
        self.tool_state.borrow().current_tool()
    }

    /// Replace the snap engine (e.g. to sync app config into the widget).
    pub fn set_snap_engine(&self, engine: SnapEngine) {
        *self.snap.borrow_mut() = engine;
        self.request_redraw();
    }

    /// Read access to the snap engine.
    pub fn snap(&self) -> std::cell::Ref<'_, SnapEngine> {
        self.snap.borrow()
    }

    /// Mutably access the snap engine.
    pub fn with_snap<R>(&self, f: impl FnOnce(&mut SnapEngine) -> R) -> R {
        f(&mut self.snap.borrow_mut())
    }

    /// Register the callback invoked when tools emit domain mutations.
    pub fn set_tool_event_handler(&self, handler: ToolEventCallback) {
        *self.tool_event.borrow_mut() = Some(handler);
    }

    /// Emit a tool event to the host app. Returns whether it was handled.
    pub(crate) fn emit_tool_event(&self, event: ToolEvent) -> bool {
        if let Some(handler) = self.tool_event.borrow().as_ref() {
            handler(event)
        } else {
            false
        }
    }

    /// Enable/disable snapping (convenience for demos).
    pub fn set_snap_enabled(&self, enabled: bool) {
        self.with_snap(|snap| snap.config_mut().enabled = enabled);
        self.request_redraw();
    }

    /// Undo the last widget-level edit. Returns true when something was undone.
    pub fn undo(&self) -> bool {
        let mut ctx = crate::widget::actions::ActionContext {
            plot: &mut self.plot.borrow_mut(),
            state: None,
            clipboard: &mut self.clipboard.borrow_mut(),
            history: None,
            config: &self.config,
        };
        let undone = self.history.borrow_mut().undo(&mut ctx).is_some();
        if undone {
            self.request_redraw();
        }
        undone
    }

    /// Redo the last undone edit. Returns true when something was redone.
    pub fn redo(&self) -> bool {
        let mut ctx = crate::widget::actions::ActionContext {
            plot: &mut self.plot.borrow_mut(),
            state: None,
            clipboard: &mut self.clipboard.borrow_mut(),
            history: None,
            config: &self.config,
        };
        let redone = self.history.borrow_mut().redo(&mut ctx).is_some();
        if redone {
            self.request_redraw();
        }
        redone
    }

    /// Whether an undo is available.
    pub fn can_undo(&self) -> bool {
        self.history.borrow().can_undo()
    }

    /// Whether a redo is available.
    pub fn can_redo(&self) -> bool {
        self.history.borrow().can_redo()
    }
    
    /// Number of undoable entries.
    pub fn history(&self) -> std::cell::Ref<'_, [crate::widget::actions::HistoryGroup]> {
        std::cell::Ref::map(self.history.borrow(), |h| h.history())
    }
    
    /// Number of redoable entries.
    pub fn redo_len(&self) -> usize {
        self.history.borrow().redo_stack().len()
    }

    /// Execute a plot action through the widget history (undoable).
    pub fn execute_action(&self, action: std::sync::Arc<dyn crate::widget::actions::PlotAction>) {
        let mut ctx = crate::widget::actions::ActionContext {
            plot: &mut self.plot.borrow_mut(),
            state: None,
            clipboard: &mut self.clipboard.borrow_mut(),
            history: None,
            config: &self.config,
        };
        self.history.borrow_mut().execute_dyn(action, &mut ctx);
        self.request_redraw();
    }
    
    /// Getter für das Link-Binding (wird in events.rs gebraucht)
    pub fn link_binding(&self) -> Option<&LinkBinding> {
        self.link_binding.as_ref()
    }

    /// Setzt die Konfiguration im Builder-Stil
    pub fn with_config(mut self, config: PlotViewConfig) -> Self {
        self.config = config;
        self
    }

    /// Registriert das Programm in der Link-Gruppe und gibt das verlinkte Programm zurück
    pub fn with_link_group(mut self, group: PlotLinkGroup, options: PlotLinkOptions) -> Self {
    // Da LinkBinding privat ist, erzeugen wir es hier direkt intern, 
    // falls Program es als Feld hält, oder wir nutzen einfach die pub-Methoden der Gruppe:
    let member_id = group.register_member();
    
    // Wenn dein struct Program ein internes link_binding-Feld hat, 
    // nutze den voll-qualifizierten Pfad innerhalb der Lib:
    self.link_binding = Some(crate::widget::link::LinkBinding {
        group,
        member_id,
        options,
    });
    self
}

    /// Immutable access to the plot model.
    pub fn plot(&self) -> std::cell::Ref<'_, Plot> {
        self.plot.borrow()
    }


  pub fn clear_cache(&self) {
        self.cache.clear();
    }
    /// Mutable access to the plot model. Invalidates the geometry cache.
    ///
    /// The closure must not call other `Program` methods that borrow the plot.
    pub fn with_plot_mut<R>(&self, f: impl FnOnce(&mut Plot) -> R) -> R {
        self.cache.clear();
        f(&mut self.plot.borrow_mut())
    }

    /// Immutable access to the plot theme.
    pub fn theme(&self) -> Theme {
        self.theme.borrow().clone()
    }

    /// Replace the plot theme. Invalidates the geometry cache.
    pub fn set_theme(&self, theme: Theme) {
        *self.theme.borrow_mut() = theme;
        self.cache.clear();
    }

    /// The view configuration used when building frames (thresholds for pins
    /// and hover, legend sizing).
    pub fn config(&self) -> &PlotViewConfig {
        &self.config
    }

    /// Mark the geometry as dirty so the next frame repaints.
    pub fn request_redraw(&self) {
        self.cache.clear();
    }
}

impl canvas::Program<Message> for Program {
    type State = PlotUiState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        // 1. Berechne die relative Mausposition innerhalb dieses Widgets
        let local_cursor = match cursor.position_in(bounds) {
            Some(local_pos) => mouse::Cursor::Available(local_pos),
            None => mouse::Cursor::Unavailable,
        };

        // 2. Setze die Bounds für die interne Berechnungslogik auf (0,0) zurück
        let local_bounds = Rectangle {
            x: 0.0,
            y: 0.0,
            ..bounds
        };

        // 3. Übergib die transformierten lokalen Daten an deinen Event-Handler
        events::handle(self, state, event, local_bounds, local_cursor)
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let geometry = self.cache.draw(renderer, bounds.size(), |frame| {
            let mut ui_state = state.clone();
            
            let local_bounds = Rectangle {
                x: 0.0,
                y: 0.0,
                ..bounds
            };

            let list = frame::build_frame(
                &mut self.plot.borrow_mut(),
                &mut ui_state,
                &self.config,
                local_bounds,
            );
            paint::paint(frame, &list.render);
        });
        vec![geometry]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        // Iced prüft hier standardmäßig mit den echten Fenster-Bounds.
        // Das lassen wir so, damit der Cursor sich nur ändert, wenn die Maus 
        // wirklich über dem jeweiligen Widget schwebt.
        if cursor.is_over(bounds) {
            match self.current_tool() {
                Tool::Select => mouse::Interaction::default(),
                Tool::Pen | Tool::Draw | Tool::Line | Tool::Curve => {
                    mouse::Interaction::Crosshair
                }
                Tool::Eraser => mouse::Interaction::NotAllowed,
                Tool::Move => mouse::Interaction::Move,
                Tool::Split => mouse::Interaction::Crosshair,
                Tool::Scale | Tool::Stretch | Tool::Join | Tool::Velocity
                | Tool::Quantize => mouse::Interaction::Crosshair,
            }
        } else {
            mouse::Interaction::default()
        }
    }
}

/// A canvas widget showing `program`'s plot.
///
/// Returns the concrete [`iced::widget::Canvas`] so callers can chain
/// `.map(...)` to embed the widget into their own message type (like the
/// iced `bezier_tool`/`multitouch` examples do).
/// A canvas widget showing `program`'s plot.
///
/// Returns the concrete [`iced::widget::Canvas`] so callers can chain
/// `.map(...)` to embed the widget into their own message type (like the
/// iced `bezier_tool`/`multitouch` examples do).
pub fn plot<P>(program: P) -> iced::widget::Canvas<P, Message>
where
    P: canvas::Program<Message>,
{
    iced::widget::Canvas::new(program)
        .width(iced::Length::Fill)  // Standardmäßig volle Breite
        .height(iced::Length::Fill) // HIER: Standardmäßig volle Höhe erlauben!
}

