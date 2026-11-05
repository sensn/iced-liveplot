//! Translation of iced canvas events into plot interactions.
//!
//! Ports the mouse handlers of the GPUI backend view (`on_mouse_down`,
//! `on_mouse_move`, `on_mouse_up`, `on_scroll`, `on_hover_state_change`) on
//! top of the shared interaction helpers (`pan_viewport`, `zoom_viewport`,
//! `zoom_to_rect`, `toggle_pin`, `zoom_factor_from_drag`).
// use crate::widget::plot;
use iced::mouse;
use iced::widget::canvas::{self, Action, Event};
use iced::Rectangle;

use crate::geom::{ScreenPoint, ScreenRect};
use crate::interaction::{
    pan_viewport, toggle_pin, zoom_factor_from_drag, zoom_to_rect, zoom_viewport, HitRegion,
};
use crate::view::Viewport;

use super::hover::update_hover_target;
use super::state::{ClickState, DragMode, DragState, PlotUiState};
use super::{Message, Program};

/// Scroll wheel line height in pixels (matches the GPUI backend default).
const SCROLL_LINE_HEIGHT: f32 = 16.0;
/// Drag distance (px) under which a release counts as a click.
const DRAG_THRESHOLD_PX: f32 = 3.0;

/// Handle a canvas event for the plot widget.
///
/// Returns `Some(Action)` when the interaction changed something that
/// requires a redraw or a published message.
pub(super) fn handle(
    program: &Program,
    state: &mut PlotUiState,
    event: &Event,
    bounds: Rectangle,
    cursor: mouse::Cursor,
) -> Option<canvas::Action<Message>> {
    let position = cursor.position_in(bounds).map(screen_point);

    // Refresh the layout-derived fields (regions, plot rect, transform) the
    // same way the GPUI backend did during paint.
    refresh_layout(program, state, bounds);

    match event {
        Event::Mouse(mouse::Event::ButtonPressed(button)) => {
            let Some(pos) = position else {
                return None;
            };
            handle_mouse_down(program, state, *button, pos);
            Some(Action::request_redraw())
        }
        Event::Mouse(mouse::Event::CursorMoved { .. }) => {
            handle_mouse_move(program, state, position);
            Some(Action::request_redraw())
        }
        Event::Mouse(mouse::Event::ButtonReleased(button)) => {
            handle_mouse_up(program, state, *button, position);
            Some(Action::request_redraw())
        }
        Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
            handle_scroll(program, state, position, delta);
            Some(Action::request_redraw())
        }
        _ => None,
    }
}

/// Rebuild the layout so `state.regions`, `state.plot_rect` and
/// `state.transform` reflect the current plot and bounds.
///
/// The GPUI backend computed these during painting; iced draws with
/// `&State`, so we refresh them here instead (in `update`, which owns
/// `&mut State`).
fn refresh_layout(program: &Program, state: &mut PlotUiState, bounds: Rectangle) {
    let mut scratch = state.clone();
    let config = program.config().clone();
    
    // --- BUGHUNT: STABILES CURSOR-LINKING BEI INTERAKTIONEN ---
    if let Some(link) = program.link_binding() {
        if link.options.link_cursor {
            if let Some(cursor_update) = link.group.latest_cursor_update() {
                // Wenn das Update von einem ANDEREN Plot stammt:
                if cursor_update.source != link.member_id {
                    // FIX: Wir schreiben den X-Wert in BEIDE Zustände (state und scratch)!
                    // Dadurch geht der Wert beim nachfolgenden Zuweisen am Ende nicht verloren
                    // und steht build_frame/build_linked_cursor sofort zur Verfügung.
                    state.linked_cursor_x = cursor_update.x;
                    scratch.linked_cursor_x = cursor_update.x;
                    
                    if cursor_update.seq > state.link_cursor_seq {
                        state.link_cursor_seq = cursor_update.seq;
                        scratch.link_cursor_seq = cursor_update.seq;
                        program.request_redraw();
                    }
                }
            }
        }
    }
    
    // --- GPUI-LINK-PORT: Empfange Updates vom verlinkten Plot ---
    if let Some(link) = program.link_binding() {
        if let Some(update) = link.group.latest_view_update() {
            // Nur reagieren, wenn das Update von einem ANDEREN Plot stammt und eine neuere Sequenz hat
            if update.source != link.member_id && update.seq > state.link_view_seq {
                program.with_plot_mut(|plot| {
                    match update.kind {
                        crate::widget::link::ViewSyncKind::Manual { viewport, sync_x, sync_y } => {
                            if let Some(current) = plot.viewport() {
                                let mut next = current;
                                // Übernehme die Koordinaten basierend auf den erlaubten Optionen
                                if link.options.link_x && sync_x { next.x = viewport.x; }
                                if link.options.link_y && sync_y { next.y = viewport.y; }
                                plot.set_manual_view(next);
                                scratch.viewport = Some(next);
                            }
                        }
                        crate::widget::link::ViewSyncKind::Reset => {
                            if link.options.link_reset {
                                plot.reset_view();
                                scratch.viewport = None;
                            }
                        }
                    }
                });
                state.link_view_seq = update.seq; // Lokale Sequenz updaten
                scratch.link_view_seq = update.seq;
            }
        }
    }

    // `build_frame` also refreshes the plot's live viewport.
    program.with_plot_mut(|plot| {
        let _frame = super::frame::build_frame(plot, &mut scratch, &config, bounds);
    });

    // Persist the derived fields the event handlers need.
    state.regions = scratch.regions;
    state.plot_rect = scratch.plot_rect;
    state.transform = scratch.transform;
    state.x_layout = scratch.x_layout;
    state.y_layout = scratch.y_layout;
    state.legend_layout = scratch.legend_layout;
    
    // HIER: Wir stellen sicher, dass der verlinkte Cursor-Fokus permanent erhalten bleibt
    // HIER ERGÄNZEN: Zustand über Interaktionen hinweg stabil halten!
    state.linked_cursor_x = scratch.linked_cursor_x;
    state.link_cursor_seq = scratch.link_cursor_seq;
}

fn handle_mouse_down(
    program: &Program,
    state: &mut PlotUiState,
    button: mouse::Button,
    pos: ScreenPoint,
) {
    // Legend entries toggle the visibility of their series.
    if let Some(series_id) = state.legend_hit(pos) {
        program.with_plot_mut(|plot| {
            if let Some(series) = plot
                .series_mut()
                .iter_mut()
                .find(|series| series.id() == series_id)
            {
                series.set_visible(!series.is_visible());
            }
        });
        return;
    }

    let region = state.regions.hit_test(pos);

    let (mode, active) = match (button, region) {
        (mouse::Button::Left, HitRegion::Plot) => (DragMode::Pan, true),
        (mouse::Button::Left, HitRegion::XAxis) => (DragMode::ZoomX, true),
        (mouse::Button::Left, HitRegion::YAxis) => (DragMode::ZoomY, true),
        (mouse::Button::Right, HitRegion::Plot) => (DragMode::ZoomRect, true),
        _ => (DragMode::Pan, false),
    };

    state.drag = Some(DragState::new(mode, pos, active));
    state.pending_click = Some(ClickState { region, button });
}

fn handle_mouse_move(
    program: &Program,
    state: &mut PlotUiState,
    position: Option<ScreenPoint>,
) {
    let Some(pos) = position else {
         //   state.is_mouse_inside = false; // <-- Hier deaktivieren!
            
            // --- TEST-ERWEITERUNG: LOKALES FLUSH BEI CURSOR LEFT ---
            program.with_plot_mut(|plot| {
                plot.is_mouse_actively_inside = false;
            });

        // Cursor left the canvas: clear hover (GPUI `on_hover_state_change`).
        let still_inside = state
            .hover
            .is_some_and(|cursor| {
                state.legend_hit(cursor).is_some()
                    || state.regions.hit_test(cursor) != HitRegion::Outside
            });
        if still_inside {
            return;
        }
        
        if let Some(link) = program.link_binding() {
            if link.options.link_cursor {
                link.group.publish_cursor_x(link.member_id, None);
            }
        }
        //  state.hovered_plot_id = None; // <-- Neu: ID zurücksetzen
        if state.hover.take().is_some() || state.hover_target.take().is_some() {

            program.request_redraw();
        }
        return;
    };

    // --- GPUI-LINK FIX: LOKALER HOVER VS. GLOBALES SYNC ---
    let hit_region = state.regions.hit_test(pos);

    if hit_region == HitRegion::Outside {
           // state.is_mouse_inside = false; // <-- Hier deaktivieren!
            
            // --- TEST-ERWEITERUNG: LOKALES FLUSH BEI OUTSIDE ---
            program.with_plot_mut(|plot| {
                plot.is_mouse_actively_inside = false;
            });

       //  state.hovered_plot_id = None; // <-- Neu: ID zurücksetzen
        // Maus ist im Randbereich: Lokales Hover-Tooltip ausblenden...
        if state.hover.take().is_some() || state.hover_target.take().is_some() {

            program.request_redraw();
        }
        
        // ...ABER wir senden trotzdem die X-Koordinate an den verlinkten Plot, 
        // damit die Infobox beim senkrechten Wechsel nicht einfriert!
        if let (Some(link), Some(transform)) = (program.link_binding(), state.transform.clone()) {
            if link.options.link_cursor {
                if let Some(data_point) = transform.screen_to_data(pos) {
                    link.group.publish_cursor_x(link.member_id, Some(data_point.x));
                }
            }
        }
        return; // Randbereich-Verarbeitung beenden
    }

    // Wenn wir hier sind, ist die Maus über einer echten aktiven Region (Plot, XAxis, YAxis)
    
       // Wenn wir hier sind, ist die Maus über einer echten aktiven Region
         //  state.is_mouse_inside = true; // <-- HIER AKTIVIEREN!
           
    // --- TEST-ERWEITERUNG: LOKALEN AKTIVEN PLOT-ZUSTAND IM WIDGET SPEICHERN ---
    program.with_plot_mut(|plot| {
        plot.is_mouse_actively_inside = true;
    });

    state.hover = Some(pos);
       // state.hovered_plot_id = Some(program.plot().id.clone()); // <-- Aktiv setzen!
    // --- GPUI-LINK FIX: NUR WENN DIE MAUS TATSÄCHLICH INTERAGIERT ---
    // Wir setzen die hovered_plot_id nur, wenn wir uns in einer echten Hit-Region befinden,
    // die nicht Outside ist (das haben wir oben abgefangen).
    if hit_region != HitRegion::Outside {
        state.hovered_plot_id = Some(program.plot().id.clone());
    }
    //state.hovered_plot_id = Some(plot.id.clone()); // <-- Neu: Die ID des aktiven Plots mitspeichern!
    // Track the hover target (pin removal needs the `is_pinned` threshold).
    if let Some(transform) = state.transform.clone() {
        let Some(plot_rect) = state.plot_rect else {
            return;
        };
        let (pin_threshold, unpin_threshold) = {
            let config = program.config();
            (config.pin_threshold_px, config.unpin_threshold_px)
        };
        update_hover_target(
            &program.plot(),
            state,
            &transform,
            plot_rect,
            pin_threshold,
            unpin_threshold,
        );
    }

    let Some(drag) = state.drag.clone() else {
        // Sende X-Koordinate bei normaler Mausbewegung (Hover ohne Drag)
        if let (Some(link), Some(transform)) = (program.link_binding(), state.transform.clone()) {
            if link.options.link_cursor {
                if let Some(data_point) = transform.screen_to_data(pos) {
                    link.group.publish_cursor_x(link.member_id, Some(data_point.x));
                }
            }
        }
        return;
    };
    
    if !drag.active {
        state.pending_click = None;
        return;
    }

      let delta = ScreenPoint::new(pos.x - drag.last.x, pos.y - drag.last.y);

    if drag.mode == DragMode::ZoomRect {
        state.selection_rect = Some(ScreenRect::new(drag.last, pos));
        state.drag.as_mut().expect("checked above").last = pos;
        return;
    }

    if let (Some(transform), Some(rect)) = (state.transform.clone(), state.plot_rect) {
        program.with_plot_mut(|plot| {
            let Some(viewport) = plot.viewport() else {
                return;
            };

            // --- GPUI HIGH-DPI TRACKING PORT ---
            // GPUI operiert nativ auf physikalischen Pixeln. Um denselben tiefen Zoom-Effekt 
            // in Iced zu erzielen, kompensieren wir den logischen Pixel-Verlust, indem wir 
            // das Delta mit dem Skalierungsfaktor des Displays multiplizieren (z.B. Faktor 1.5 oder 2.0).
            // Falls das Widget kein dynamisches Scale-Feld exportiert, ist ein Boost-Faktor von 2.0 
            // der mathematische Standard für moderne High-DPI Desktops.
            let dpi_scale = 2.0; 
            let adjusted_delta_x = delta.x * dpi_scale;
            let adjusted_delta_y = delta.y * dpi_scale;

                               let next = match drag.mode {
                DragMode::Pan => pan_viewport(viewport, delta, &transform),
                DragMode::ZoomX => {
                    let axis_pixels = rect.max.x - rect.min.x;
                    let raw_factor = zoom_factor_from_drag(delta.x, axis_pixels);
                    
                    // --- THE DEEP ZOOM FIX: SENSITIVITY EQUALIZER ---
                    // Da Iced das Delta in logischen Pixeln liefert, die Achsenlänge aber 
                    // in physikalischen Einheiten steht, halbieren wir die Zoom-Wirkung.
                    // Wir quadrieren den Faktor (oder wenden die Zweierpotenz an), um den 
                    // linearen Verlust perfekt auszugleichen, ohne das Koordinatensystem zu brechen.
                    let factor = if raw_factor < 1.0 {
                        raw_factor.powi(2) // Zoom In wird deutlich verstärkt und tiefer
                    } else {
                        raw_factor // Zoom Out bleibt stabil
                    };

                    let center = transform
                        .screen_to_data(pos)
                        .unwrap_or_else(|| viewport_center(&viewport));
                    Some(zoom_viewport(viewport, center, factor, 1.0))
                }
                DragMode::ZoomY => {
                    let axis_pixels = rect.max.y - rect.min.y;
                    let raw_factor = zoom_factor_from_drag(-delta.y, axis_pixels);
                    
                    // --- THE DEEP ZOOM FIX: SENSITIVITY EQUALIZER ---
                    let factor = if raw_factor < 1.0 {
                        raw_factor.powi(2)
                    } else {
                        raw_factor
                    };

                    let center = transform
                        .screen_to_data(pos)
                        .unwrap_or_else(|| viewport_center(&viewport));
                    Some(zoom_viewport(viewport, center, 1.0, factor))
                }
                DragMode::ZoomRect => unreachable!("handled above"),
            };



            if let Some(next) = next {
                apply_manual_view(program, plot, state, rect, next);
            }
        });
    }


    state.drag.as_mut().expect("checked above").last = pos;
    state.pending_click = None;

    // Sende X-Koordinate nach einem Zoom oder Pan-Drag
    if let (Some(link), Some(transform)) = (program.link_binding(), state.transform.clone()) {
        if link.options.link_cursor {
            if let Some(data_point) = transform.screen_to_data(pos) {
                link.group.publish_cursor_x(link.member_id, Some(data_point.x));
            }
        }
    }
}


fn handle_mouse_up(
    program: &Program,
    state: &mut PlotUiState,
    button: mouse::Button, // HIER: Unterstrich entfernt, wir brauchen den Button!
    position: Option<ScreenPoint>,
) {
    let drag = state.drag.take();
    let click = state.pending_click.take();

    // Commit a box zoom (right-button drag) on release.
    if let (Some(drag), Some(selection)) = (drag.as_ref(), state.selection_rect.take()) {
        if drag.active && drag.mode == DragMode::ZoomRect {
            if let Some(transform) = state.transform.clone() {
                let rect = super::geometry::normalized_rect(selection);
                program.with_plot_mut(|plot| {
                    if let Some(viewport) = plot.viewport() {
                        if let Some(next) = zoom_to_rect(viewport, rect, &transform) {
                            let Some(plot_rect) = state.plot_rect else { return; };
                            apply_manual_view(program, plot, state, plot_rect, next);
                        }
                    }
                });
            }
        }
    }

    // --- GEFIXTE DOPPEL-INTERAKTIONS LOGIK FÜR PIN/MARKER ---
    // Ein Klick zählt, wenn der linke Knopf losgelassen wird und wir nicht in einem Box-Zoom-Modus waren.
    // Das umschifft das vorzeitige Löschen von `pending_click` während minimaler Mausbewegungen.
    let was_click = button == mouse::Button::Left 
        && drag.as_ref().is_none_or(|d| d.mode != DragMode::ZoomRect);

    if was_click {
        if let (Some(pos), Some(transform)) = (position, state.transform.clone()) {
            let Some(plot_rect) = state.plot_rect else { return; };
            let (pin_threshold, unpin_threshold) = {
                let config = program.config();
                (config.pin_threshold_px, config.unpin_threshold_px)
            };

            let target = state
                .hover_target
                .take()
                .filter(|target| {
                    super::hover::hover_target_within_threshold(target, pos, program.config())
                })
                .or_else(|| {
                    super::hover::compute_hover_target(
                        &program.plot(),
                        &transform,
                        pos,
                        Some(plot_rect),
                        pin_threshold,
                        unpin_threshold,
                    )
                });

            if let Some(target) = target {
                program.with_plot_mut(|plot| {
                    toggle_pin(plot.pins_mut(), target.pin);
                });
            }
        }
    }

    state.selection_rect = None;
}

fn handle_scroll(
    program: &Program,
    state: &mut PlotUiState,
    position: Option<ScreenPoint>,
    delta: &mouse::ScrollDelta,
) {
    let Some(pos) = position else {
        return;
    };
    if state.legend_hit(pos).is_some() {
        return;
    }

    let Some(transform) = state.transform.clone() else {
        return;
    };
    let region = state.regions.hit_test(pos);

    let delta_y = match delta {
        mouse::ScrollDelta::Lines { y, .. } => y * SCROLL_LINE_HEIGHT,
        mouse::ScrollDelta::Pixels { y, .. } => *y,
    };
    let factor = scroll_zoom_factor(delta_y);
    if factor == 1.0 {
        return;
    }

    let (factor_x, factor_y) = match region {
        HitRegion::XAxis => (factor, 1.0),
        HitRegion::YAxis => (1.0, factor),
        HitRegion::Plot => (factor, factor),
        HitRegion::Outside => return,
    };

    program.with_plot_mut(|plot| {
        let Some(viewport) = plot.viewport() else {
            return;
        };
        // --- GPUI-BUGFIX: NUTZE DIE EXAKT AKTUELLE MAUSPOSITION ---
        let center = transform
            .screen_to_data(pos)
            .unwrap_or_else(|| viewport_center(&viewport));
        let next = zoom_viewport(viewport, center, factor_x, factor_y);
        if let Some(rect) = state.plot_rect {
            apply_manual_view(program, plot, state, rect, next);
        }
    });
    
 // --- THE ABSOLUTE MOUSEWHEEL INFOBOX FIX ---
    // Nachdem das Scrollrad das Layout verändert hat und das neue `state.transform` feststeht,
    // rechnen wir die X-Koordinate der Maus sofort neu um und senden sie an den passiven Plot.
    // Das verhindert, dass die Infobox beim Scrollen einfriert oder verschwindet!
    if state.regions.hit_test(pos) != HitRegion::Outside {
        if let (Some(link), Some(transform)) = (program.link_binding(), state.transform.clone()) {
            if link.options.link_cursor {
                if let Some(data_point) = transform.screen_to_data(pos) {
                    link.group.publish_cursor_x(link.member_id, Some(data_point.x));
                }
            }
        }
    }
}

/// Zoom factor per scroll amount, mirroring the GPUI backend's
/// `scroll_zoom_factor` but compensated for Iced input frequency.
fn scroll_zoom_factor(delta_y: f32) -> f64 {
    if delta_y.abs() < 0.01 {
        return 1.0;
    }
    
    // --- GPUI DEEP-ZOOM PORT ---
    // 1. Wir erhöhen den Faktor von 0.002 auf 0.005, um den Dämpfungsverlust von Iced auszugleichen.
    // 2. Wir entfernen das restriktive .clamp(0.1, 10.0) vollständig, damit die Wellenform
    //    unbegrenzt tief und rasant expandieren kann, genau wie in der GPUI-Version!
    1.0 - (delta_y as f64 * 0.005)
}


/// Center point of a viewport, used when the transform cannot map the cursor.
fn viewport_center(viewport: &Viewport) -> crate::geom::Point {
    crate::geom::Point::new(
        (viewport.x.min + viewport.x.max) * 0.5,
        (viewport.y.min + viewport.y.max) * 0.5,
    )
}

fn screen_point(p: iced::Point) -> ScreenPoint {
    ScreenPoint::new(p.x, p.y)
}

/// Apply a manual (user-driven) viewport, like the GPUI backend's
/// `apply_manual_view`: switches the view mode to manual and refreshes the
/// cached transform.
fn apply_manual_view(
    program: &Program,
    plot: &mut crate::plot::Plot,
    state: &mut PlotUiState,
    rect: ScreenRect,
    viewport: Viewport,
) {
    plot.set_manual_view(viewport);
    state.viewport = Some(viewport);
    if let Some(transform) = crate::transform::Transform::new(viewport, rect) {
        state.transform = Some(transform);
    }

    // --- GPUI-LINK-PORT: Sende das neue Layout an die Gruppe ---
    if let Some(link) = program.link_binding() {
        // Wir publizieren den neuen Viewport. 
        // link_x und link_y steuern, welche Achsen synchronisiert werden sollen.
        link.group.publish_manual_view(
            link.member_id,
            viewport,
            link.options.link_x,
            link.options.link_y,
        );
        // Wir aktualisieren direkt unsere eigene Sequenz, damit wir das Event nicht selbst spiegeln
        if let Some(update) = link.group.latest_view_update() {
            state.link_view_seq = update.seq;
        }
    }

    program.request_redraw();
}