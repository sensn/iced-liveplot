//! The iced backend: a [`canvas::Program`] that renders a [`Plot`] and
//! translates mouse events into plot interactions (pan, zoom, pins, hover,
//! box-select, legend toggles).

pub mod config;
pub mod constants;
pub mod frame;
pub mod geometry;
pub mod hover;
pub mod link;
pub mod paint;
pub mod state;
pub mod text;

mod events;

use std::cell::RefCell;

use iced::mouse;
use iced::widget::canvas::{self, Cache, Event, Geometry};
use iced::Rectangle;

use crate::plot::Plot;
use crate::style::Theme;

pub use config::PlotViewConfig;
use state::PlotUiState;
// linking
use crate::widget::link::PlotLinkOptions;
use crate::widget::link::{PlotLinkGroup, LinkBinding};

/// A message produced by the plot widget.
#[derive(Debug, Clone)]
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
}

impl Program {
    pub fn new(plot: Plot) -> Self {
        Self {
            plot: RefCell::new(plot),
            theme: RefCell::new(Theme::default()),
            config: PlotViewConfig::default(),
            cache: Cache::new(),
            link_binding: None, // Initial leer
        }
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
            mouse::Interaction::Crosshair
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

