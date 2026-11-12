//! Translation of iced canvas events into plot interactions.
//!
//! Ports the mouse handlers of the GPUI backend view (`on_mouse_down`,
//! `on_mouse_move`, `on_mouse_up`, `on_scroll`, `on_hover_state_change`) on
//! top of the shared interaction helpers (`pan_viewport`, `zoom_viewport`,
//! `zoom_to_rect`, `toggle_pin`, `zoom_factor_from_drag`).
// use crate::widget::plot;
use std::time::Instant;
use iced::mouse;
//use iced::mouse::{self, Click, Cursor, Interaction, click::Kind},
use iced::widget::canvas::{self, Action, Event};
use iced::{Point, Rectangle};

use crate::geom::{ScreenPoint, ScreenRect};
use crate::interaction::{
    pan_viewport, toggle_pin, zoom_factor_from_drag, zoom_to_rect, zoom_viewport, HitRegion,
};
use crate::view::Viewport;

use super::hover::update_hover_target;
use super::state::{ClickState, DragMode, DragState, LastClick, PlotUiState};
use super::tools::{MoveGesture, ToolGesture, ToolState, TrimZone};
use super::{Message, MidiEdit, NoteTargets, Program, ResizeEdge, Tool, ToolEvent};

/// Scroll wheel line height in pixels (matches the GPUI backend default).
const SCROLL_LINE_HEIGHT: f32 = 16.0;
/// Drag distance (px) under which a release counts as a click.
const DRAG_THRESHOLD_PX: f32 = 3.0;
/// Maximum distance (px) between clicks for a double-click.
const DOUBLE_CLICK_THRESHOLD_PX: f32 = 6.0;
/// Maximum time (ms) between clicks for a double-click.
const DOUBLE_CLICK_THRESHOLD_MS: u128 = 300;

/// Check if a new click forms a double-click with the last click
fn is_double_click(last: &LastClick, pos: ScreenPoint, button: mouse::Button) -> bool {
    let now = Instant::now();
    last.is_double_click(pos, button, now)
}

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
            handle_mouse_up(program, state, *button, position)
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
        let _frame = super::frame::build_frame(program, plot, &mut scratch, &config, bounds);
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
    // Regions linked to the series (DataRegion::series == series_id) follow.
    if let Some(series_id) = state.legend_hit(pos) {
        program.with_plot_mut(|plot| {
            let now_visible = plot
                .series_mut()
                .iter_mut()
                .find(|series| series.id() == series_id)
                .map(|series| {
                    series.set_visible(!series.is_visible());
                    series.is_visible()
                })
                .unwrap_or(false);
            // Keep linked regions in sync with their parent series.
            for region in plot.regions_mut() {
                if region.series == Some(series_id) {
                    region.visible = now_visible;
                }
            }
        });
        program.request_redraw();
        return;
    }

    let region = state.regions.hit_test(pos);

    // ---- PLAYHEAD: click/drag on the X axis moves the transport playhead ----
    if button == mouse::Button::Left && region == HitRegion::XAxis {
        if let Some(transform) = state.transform.clone() {
            if let Some(point) = transform.screen_to_data(pos) {
                let ticks = (point.x * program.ticks_per_second()).round().max(0.0) as u64;
                program.set_playhead_ticks(ticks);
                return;
            }
        }
    }

    // ---- TOOLS: active tool hijacks left-drag inside the plot area ----
    // Grab is the default navigation tool and keeps the ORIGINAL behavior:
    // left-drag pans, click-on-data pins, right-drag box zooms (below).
    if button == mouse::Button::Left && region == HitRegion::Plot {
        let tool = program.current_tool();
        if tool != Tool::Grab {
            run_tool_mouse(program, state, tool, pos, ToolPhase::Down);
            // Tools consume the interaction completely. The Select tool's
            // BoxSelect drag (started above) must NOT be overwritten below,
            // otherwise panning replaces the rubber band on the first move.
            if tool == Tool::Select {
                state.pending_click = Some(ClickState { region, button });
                return;
            }
            return;
        }
    }

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

    // Bestimme exakt, in welchem Bereich des Widgets sich die Maus befindet
    let hit_region = state.regions.hit_test(pos);

    // --- PRÄZISE HITREGION-WEICHE MIT INTEGRATION DES UNGETEILTEN PLOT-FLAGS ---
    match hit_region {
        HitRegion::Plot => {

            //  state.is_mouse_inside = true; // <-- HIER AKTIVIEREN!
            state.hover = Some(pos);
            
            // Lokalen, ungeteilten Plot-Zustand exklusiv für das Signal-Rechteck aktivieren
            program.with_plot_mut(|plot| {
                plot.is_mouse_actively_inside = true;
            });

            // Track the hover target (pin removal needs the `is_pinned` threshold).
            if let Some(transform) = state.transform.clone() {
                if let Some(plot_rect) = state.plot_rect {
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
            }
        }
        HitRegion::XAxis | HitRegion::YAxis => {
            // Maus ist auf den Achsen: Ausblenden des Kurvenhovers,
            // aber das Flag wird auf false gesetzt, da wir uns außerhalb der Signalfläche befinden!
            program.with_plot_mut(|plot| {
                plot.is_mouse_actively_inside = false;
            });

            if state.hover_target.take().is_some() {
                program.request_redraw();
            }
            state.hover = Some(pos);
        }
        HitRegion::Outside => {
            // Maus ist komplett im ungenutzten Außenbereich/Rand
            // state.is_mouse_inside = false; // <-- Hier deaktivieren!
            
            program.with_plot_mut(|plot| {
                plot.is_mouse_actively_inside = false;
            });

            //  state.hovered_plot_id = None; // <-- Neu: ID zurücksetzen
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
    }

    // --- GPUI-LINK FIX: NUR WENN DIE MAUS TATSÄCHLICH INTERAGIERT ---
    if hit_region != HitRegion::Outside {
        state.hovered_plot_id = Some(program.plot().id.clone());
    }

    // ---- TOOLS: feed move to active tool while dragging ----
    // Selection/move/scale/stretch gestures live in the tool state (not in
    // `state.drag`), so "dragging" means either an active region drag OR an
    // active tool gesture (box select).
    let drag_active = state.drag.as_ref().is_some_and(|d| d.active);
    let gesture_active = program.with_tool_state(|ts| {
        ts.drag_move.active || matches!(ts.gesture, super::tools::ToolGesture::Draw { .. })
    });
    let tool = program.current_tool();
    let tool_dragging = drag_active || gesture_active;
    if tool_dragging && hit_region == HitRegion::Plot && tool != Tool::Grab {
        run_tool_mouse(program, state, tool, pos, ToolPhase::Move);
        if matches!(tool, Tool::Draw | Tool::Select | Tool::Move | Tool::Scale | Tool::Stretch) {
            // These tools consume the move; no pan
            state.pending_click = None;
            return;
        }
    }

    // --- DRAG & VIEWPORT-MANIPULATION ---
    let Some(drag) = state.drag.clone() else {
        // Kein aktiver Drag: Sende Standard-Hover-Koordinaten an die verlinkte Gruppe
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

            let dpi_scale = 2.0; 
            let _adjusted_delta_x = delta.x * dpi_scale;
            let _adjusted_delta_y = delta.y * dpi_scale;

            let next = match drag.mode {
                DragMode::Pan => pan_viewport(viewport, delta, &transform),
                DragMode::ZoomX => {
                    let axis_pixels = rect.max.x - rect.min.x;
                    let raw_factor = zoom_factor_from_drag(delta.x, axis_pixels);
                    let factor = if raw_factor < 1.0 { raw_factor.powi(2) } else { raw_factor };
                    let center = transform.screen_to_data(pos).unwrap_or_else(|| viewport_center(&viewport));
                    Some(zoom_viewport(viewport, center, factor, 1.0))
                }
                DragMode::ZoomY => {
                    let axis_pixels = rect.max.y - rect.min.y;
                    let raw_factor = zoom_factor_from_drag(-delta.y, axis_pixels);
                    let factor = if raw_factor < 1.0 { raw_factor.powi(2) } else { raw_factor };
                    let center = transform.screen_to_data(pos).unwrap_or_else(|| viewport_center(&viewport));
                    Some(zoom_viewport(viewport, center, 1.0, factor))
                }
                DragMode::ZoomRect | DragMode::BoxSelect => unreachable!("handled above"),
            };

            if let Some(next) = next {
                apply_manual_view(program, plot, state, rect, next);
            }
        });
    }

    state.drag.as_mut().expect("checked above").last = pos;
    state.pending_click = None;

    // Synchronisiere X-Achse nach Pan/Zoom im verlinkten Verbund
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
    button: mouse::Button,
    position: Option<ScreenPoint>,
) -> Option<canvas::Action<Message>> {
    let drag = state.drag.take();
    let _click = state.pending_click.take();

    // ---- TOOLS: finish tool gesture (before zoom-rect / double-click logic) ----
    // Grab keeps the original flow: box-zoom commit, double-click ResetView,
    // pin toggle — all handled below.
    let tool = program.current_tool();
    if tool != Tool::Grab && button == mouse::Button::Left {
        if let Some(pos) = position {
            let was_box = state.drag.as_ref().is_some_and(|d| d.mode == DragMode::BoxSelect && d.active);
            run_tool_mouse(program, state, tool, pos, ToolPhase::Up);
            if tool != Tool::Select || was_box {
                state.selection_rect = None;
                state.last_click = None;
                state.last_pin_toggle = None;
                return Some(canvas::Action::request_redraw());
            }
        }
    }

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

    // --- DOUBLE-CLICK DETECTION FOR RESETVIEW (Grab tool only) ---
    // ResetView is EXCLUSIVE to the Grab (default navigation) tool and only on
    // a DOUBLE-click over EMPTY plot space. Every other tool, every single
    // click and any click on data must never reset the view.
    let is_grab = program.current_tool() == Tool::Grab;
    let was_click = button == mouse::Button::Left
        && drag.as_ref().is_none_or(|d| d.mode != DragMode::ZoomRect)
        && is_grab;

    if was_click {
        if let Some(pos) = position {
            // Does this click land on a data target (series point / pin / region)?
            // Double-click on empty space resets the view; double-click ON data
            // only toggles the pin (no ResetView) — matching DAW conventions.
            let on_data = state
                .hover_target
                .as_ref()
                .is_some_and(|t| {
                    super::hover::hover_target_within_threshold(t, pos, program.config())
                })
                || plot_event_under_cursor(program, state, pos);
            // Check if this is a double-click with the previously stored click
            if let Some(last_click) = state.last_click {
                if is_double_click(&last_click, pos, button) && !on_data {
                    // Double-click detected: UNDO the pin from the first click, then emit ResetView
                    if let Some(pin_toggle) = state.last_pin_toggle.take() {
                        program.with_plot_mut(|plot| {
                            let pins = plot.pins_mut();
                            if pin_toggle.added {
                                // Pin was added by first click, remove it
                                if let Some(index) = pins.iter().position(|p| *p == pin_toggle.pin) {
                                    pins.swap_remove(index);
                                }
                            } else {
                                // Pin was removed by first click, add it back
                                pins.push(pin_toggle.pin);
                            }
                        });
                    }
                    state.last_click = None;
                    state.last_pin_toggle = None;
                    return Some(canvas::Action::publish(Message::ResetView));
                } else {
                    // Not a double-click: execute pin for THIS click immediately, 
                    // and store it as the new "last click" for potential future double-click
                    execute_pin_and_store(program, state, pos, button);
                    return Some(canvas::Action::request_redraw());
                }
            } else {
                // First click: execute pin immediately and store for potential double-click undo
                execute_pin_and_store(program, state, pos, button);
                return Some(canvas::Action::request_redraw());
            }
        }
    } else {
        // If it was a drag or right-click, clear the last_click but KEEP the pin (don't undo)
        state.last_click = None;
        state.last_pin_toggle = None;
    }

    state.selection_rect = None;
    Some(canvas::Action::request_redraw())
}

/// Execute pin toggle immediately and store the result for potential double-click undo.
fn execute_pin_and_store(
    program: &Program,
    state: &mut PlotUiState,
    pos: ScreenPoint,
    button: mouse::Button,
) {
    if let (Some(transform), Some(plot_rect)) = (state.transform.clone(), state.plot_rect) {
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
            // Execute the pin toggle and capture whether it was added or removed
            let added = program.with_plot_mut(|plot| {
                toggle_pin(plot.pins_mut(), target.pin)
            });
            
            // Store for potential double-click undo
            state.last_pin_toggle = Some(super::state::PinToggle {
                pin: target.pin,
                added,
                at: Instant::now(),
                screen_pos: pos,
            });
        }
    }
    
    // Store this click for double-click detection
    state.last_click = Some(LastClick {
        position: pos,
        button,
        time: Instant::now(),
    });
}

/// True when the cursor is over a region (MIDI note, clip, marker, ...).
/// Used to suppress ResetView on double-click over data.
fn plot_event_under_cursor(
    program: &Program,
    state: &PlotUiState,
    pos: crate::geom::ScreenPoint,
) -> bool {
    let Some(transform) = state.transform.clone() else {
        return false;
    };
    let Some(data) = transform.screen_to_data(pos) else {
        return false;
    };
    let Some(plot_rect) = state.plot_rect else {
        return false;
    };
    if !super::geometry::rect_contains(plot_rect, pos) {
        return false;
    }
    region_at(program, &transform, data).is_some()
}

/// Phase of a tool mouse interaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolPhase {
    Down,
    Move,
    Up,
}

/// Feed a mouse event to the active tool.
///
/// Converts the screen position to data space using the current transform,
/// applies snapping, tracks the gesture in `ToolState` and emits
/// [`ToolEvent`]s to the host application via the registered callback.
pub(crate) fn run_tool_mouse(
    program: &Program,
    state: &mut PlotUiState,
    tool: Tool,
    pos: ScreenPoint,
    phase: ToolPhase,
) {
    let Some(transform) = state.transform.clone() else {
        return;
    };
    let Some(plot_rect) = state.plot_rect else {
        return;
    };
    let inside = super::geometry::rect_contains(plot_rect, pos);
    // Starting a gesture outside the plot is meaningless, but a drag that
    // wanders out (for instance shrinking a note past the left edge) must keep
    // tracking so the release can still commit — therefore the Up phase always
    // converts, while Down/Move require the cursor to be inside.
    if !inside && phase != ToolPhase::Up {
        return;
    }

    let raw_point = transform.screen_to_data(pos).unwrap_or(crate::geom::Point::new(0.0, 0.0));

    // Snap the point using the program's snap engine. The pen, move and trim
    // gestures all snap so edits land on the musical grid.
    let (snapped_point, _did_snap) = if inside {
        program.with_snap(|snap| {
            let regions: Vec<crate::region::DataRegion> = program.plot().regions().to_vec();
            let view_range = program
                .plot()
                .viewport()
                .map(|v| crate::region::Range::new(v.x.min, v.x.max))
                .unwrap_or(crate::region::Range::new(0.0, 1.0));
            let result = snap.snap(raw_point, &regions, &transform, view_range, 120.0);
            (result.point, result.snapped)
        })
    } else {
        (raw_point, false)
    };

    let screen = pos;

    // ---- SELECT tool: rubber band starts on down, extends on move, commits on up ----
    if tool == Tool::Select {
        match phase {
            ToolPhase::Down => {
                state.drag = Some(DragState::new(DragMode::BoxSelect, pos, true));
                state.selection_rect = Some(crate::geom::ScreenRect::new(pos, pos));
            }
            ToolPhase::Move => {
                if let Some(d) = state.drag.as_mut() {
                    if d.mode == DragMode::BoxSelect {
                        d.last = pos;
                        state.selection_rect = Some(crate::geom::ScreenRect::new(d.start, pos));
                    }
                }
            }
            ToolPhase::Up => {
                if let (Some(rect), Some(transform)) = (state.selection_rect.take(), state.transform.clone()) {
                    let norm = super::geometry::normalized_rect(rect);
                    if let (Some(p0), Some(p1)) = (
                        transform.screen_to_data(norm.min),
                        transform.screen_to_data(norm.max),
                    ) {
                        let (x0, x1) = if p0.x <= p1.x { (p0.x, p1.x) } else { (p1.x, p0.x) };
                        let (y0, y1) = if p0.y <= p1.y { (p0.y, p1.y) } else { (p1.y, p0.y) };
                        // The host marks regions selected; mirror that list into
                        // the tool state so group edits apply to all of them.
                        program.with_tool_state(|ts| {
                            ts.selected_regions = program
                                .plot()
                                .regions()
                                .iter()
                                .filter(|r| r.selected)
                                .map(|r| r.id)
                                .collect();
                        });
                        program.emit_tool_event(ToolEvent::BoxSelect { x0, x1, y0, y1 });
                    }
                }
                state.drag = None;
            }
        }
        return;
    }

    // ---- MOVE / SCALE / STRETCH: zone based, DAW semantics ----
    //
    // Pointer zones inside an event (Logic Pro / Ableton style):
    //   * left strip  ~TRIM_ZONE_PX          → trim the START edge
    //   * right strip ~TRIM_ZONE_PX          → trim the END edge
    //   * middle                              → MOVE (event or whole selection)
    // * empty space never starts any gesture (panning stays with Grab);
    // * Scale/Stretch are dedicated edge tools that always trim that edge
    //   (they ignore the zone);
    // * Move uses the zone under the cursor.
    if matches!(tool, Tool::Move | Tool::Scale | Tool::Stretch) {
        let forced_zone = match tool {
            Tool::Scale => Some(TrimZone::Start),
            Tool::Stretch => Some(TrimZone::End),
            _ => None,
        };
        let zone_to_edge = |zone: TrimZone| -> Option<ResizeEdge> {
            match zone {
                TrimZone::Start => Some(ResizeEdge::Start),
                TrimZone::End => Some(ResizeEdge::End),
                TrimZone::Move => None,
            }
        };
        match phase {
            ToolPhase::Down => {
                // A gesture needs a region under the pointer.
                let Some(region) = region_at(program, &transform, snapped_point) else {
                    return;
                };
                let zone = forced_zone.or_else(|| {
                    TrimZone::zone_for(pos, &region, &transform)
                });
                // No zone (cursor outside the region) → inert gesture.
                let Some(zone) = zone else {
                    return;
                };
                let primary = region.clone();
                // Targets: the pressed region plus every selected region.
                let mut targets: Vec<crate::region::DataRegion> = vec![primary.clone()];
                for r in program.plot().regions().iter() {
                    if r.selected && r.id != primary.id {
                        targets.push(r.clone());
                    }
                }
                let grab = snapped_point;
                program.with_tool_state(|ts| {
                    ts.drag_move = MoveGesture {
                        primary: Some(primary.id),
                        grab,
                        current: grab,
                        zone: Some(zone),
                        targets: targets
                            .iter()
                            .filter_map(|r| r.time_range().map(|t| (r.id, t.min, t.max)))
                            .collect(),
                        active: true,
                    };
                    ts.gesture = ToolGesture::None;
                });
            }
            ToolPhase::Move => {
                program.with_tool_state(|ts| {
                    if ts.drag_move.active {
                        ts.drag_move.current = snapped_point;
                    }
                });
            }
            ToolPhase::Up => {
                // The release position is authoritative: the last motion
                // event may be coalesced or dropped.
                let release = snapped_point;
                let gesture = program.with_tool_state(|ts| {
                    let mut g = std::mem::take(&mut ts.drag_move);
                    if g.active {
                        g.current = release;
                    }
                    g
                });
                let Some(primary_id) = gesture.primary else {
                    return;
                };
                if !gesture.active {
                    return;
                }
                let dx = gesture.dx();
                let dy = gesture.dy();
                if dx.abs() < 1e-9 && dy.abs() < 1e-9 {
                    return; // click without drag → nothing
                }
                // Resolve the gesture back to live regions.
                let targets: Vec<crate::region::DataRegion> = gesture
                    .targets
                    .iter()
                    .filter_map(|(id, _, _)| {
                        program.plot().regions().iter().find(|r| r.id == *id).cloned()
                    })
                    .collect();
                let tick_of = |seconds: f64| -> i64 {
                    (seconds * program.ticks_per_second()).round() as i64
                };
                // Model indices live in the region metadata (`note_index`),
                // written by the host when it mirrors its model into regions.
                let model_indices: Vec<usize> = targets
                    .iter()
                    .filter_map(|r| {
                        r.metadata
                            .get("note_index")
                            .and_then(|v| v.as_u64())
                            .map(|i| i as usize)
                    })
                    .collect();
                if model_indices.is_empty() {
                    let _ = primary_id;
                    return; // not a MIDI region — nothing to edit
                }
                let targets = NoteTargets { indices: model_indices };
                let dt_ticks = tick_of(dx);
                let dy_semi = dy.round().clamp(-127.0, 127.0) as i8;
                match zone_to_edge(gesture.zone.unwrap_or(TrimZone::Move)) {
                    // Middle (or a move gesture) → move the whole group.
                    None => {
                        program.emit_tool_event(ToolEvent::Midi(MidiEdit::Move {
                            targets,
                            dt_ticks,
                            dy_semitones: dy_semi,
                        }));
                    }
                    // Left/right strip → trim the group by the cursor travel.
                    Some(edge) => {
                        program.emit_tool_event(ToolEvent::Midi(MidiEdit::Resize {
                            targets,
                            edge,
                            dt_ticks,
                        }));
                    }
                }
            }
        }
        return;
    }

    // ---- Other tools: generic gesture tracking + finish ----
    program.with_tool_state(|ts| {
        match phase {
            ToolPhase::Down => {
                match tool {
                    Tool::Pen | Tool::Eraser | Tool::Split => ts.start_click(snapped_point, screen),
                    Tool::Join | Tool::Velocity | Tool::Quantize => {
                        ts.start_drag(snapped_point, screen)
                    }
                    Tool::Draw => ts.start_draw(snapped_point),
                    _ => {}
                }
            }
            ToolPhase::Move => {
                match ts.gesture_mut() {
                    ToolGesture::Drag { current, current_screen, .. } => {
                        *current = snapped_point;
                        *current_screen = screen;
                    }
                    ToolGesture::Draw { points } => {
                        points.push(snapped_point);
                    }
                    _ => {}
                }
            }
            ToolPhase::Up => {
                finish_tool_gesture(program, tool, ts, &transform);
            }
        }
    });
}

/// Complete the pending gesture for `tool` and emit domain events.
fn finish_tool_gesture(
    program: &Program,
    tool: Tool,
    ts: &mut ToolState,
    transform: &crate::transform::Transform,
) {
    let _ = transform;
    match tool {
        Tool::Pen => {
            if let ToolGesture::Click { position, .. } =
                std::mem::replace(&mut ts.gesture, ToolGesture::None)
            {
                emit_domain_pen(program, position);
            }
        }
        
        Tool::Eraser => {
            if let ToolGesture::Click { position, .. } =
                std::mem::replace(&mut ts.gesture, ToolGesture::None)
            {
                // Erase the clicked region; if it belongs to the box selection
                // the WHOLE selection is erased (DAW behaviour).
                if let Some(region) = region_at(program, transform, position) {
                    let mut targets: Vec<crate::region::DataRegion> = vec![region.clone()];
                    for r in program.plot().regions().iter() {
                        if r.selected && r.id != region.id {
                            targets.push(r.clone());
                        }
                    }
                    let indices: Vec<usize> = targets
                        .iter()
                        .filter_map(|r| {
                            r.metadata
                                .get("note_index")
                                .and_then(|v| v.as_u64())
                                .map(|i| i as usize)
                        })
                        .collect();
                    if !indices.is_empty() {
                        program.emit_tool_event(ToolEvent::Midi(MidiEdit::Delete {
                            targets: NoteTargets { indices },
                        }));
                        return;
                    }
                }
                erase_at(program, transform, position);
            }
        }
        Tool::Split => {
            if let ToolGesture::Click { position, .. } =
                std::mem::replace(&mut ts.gesture, ToolGesture::None)
            {
                if let Some(region) = region_at(program, transform, position) {
                    // Split the clicked note, or every selected one.
                    let mut targets: Vec<crate::region::DataRegion> = vec![region.clone()];
                    for r in program.plot().regions().iter() {
                        if r.selected && r.id != region.id {
                            targets.push(r.clone());
                        }
                    }
                    let indices: Vec<usize> = targets
                        .iter()
                        .filter_map(|r| {
                            r.metadata
                                .get("note_index")
                                .and_then(|v| v.as_u64())
                                .map(|i| i as usize)
                        })
                        .collect();
                    if !indices.is_empty() {
                        let tick = (position.x * program.ticks_per_second()).round().max(0.0) as u64;
                        program.emit_tool_event(ToolEvent::Midi(MidiEdit::Split {
                            targets: NoteTargets { indices },
                            tick,
                        }));
                        return;
                    }
                    program.emit_tool_event(ToolEvent::SplitRegion {
                        region: region.id,
                        time: position.x,
                    });
                }
            }
        }
        Tool::Join | Tool::Velocity | Tool::Quantize => {
            program.with_tool_state(|t| t.gesture = ToolGesture::None);
        }
        _ => {
            ts.gesture = ToolGesture::None;
        }
    }
}

/// Emit the domain-specific pen insertion event for a click at `position`.
fn emit_domain_pen(program: &Program, position: crate::geom::Point) {
    match program.domain() {
        crate::widget::Domain::Midi => {
            let pitch = position.y.round().clamp(0.0, 127.0) as u8;
            let bpm = 120.0;
            let duration = program.note_length().seconds(bpm);
            program.emit_tool_event(ToolEvent::InsertMidiNote {
                pitch,
                velocity: 100,
                start: position.x,
                duration,
            });
        }
        crate::widget::Domain::Audio => {
            let bpm = 120.0;
            let duration = program.note_length().seconds(bpm) * 4.0;
            program.emit_tool_event(ToolEvent::InsertAudioClip {
                start: position.x,
                duration,
            });
        }
        crate::widget::Domain::Automation => {
            program.emit_tool_event(ToolEvent::InsertAutomationPoint {
                time: position.x,
                value: position.y.clamp(0.0, 1.0),
            });
        }
        crate::widget::Domain::Markers => {
            program.emit_tool_event(ToolEvent::InsertMarker { time: position.x });
        }
        crate::widget::Domain::Loop => {
            program.emit_tool_event(ToolEvent::Notify(format!(
                "Loop: click start at {:.2}s (drag with Move to set end)",
                position.x
            )));
        }
    }
}

/// Find the region under the cursor (topmost, unlocked, visible).
/// Topmost visible region under the cursor.
///
/// Uses the full 2D hit test (`DataRegion::contains`) so a MIDI note is only
/// hit inside its pitch row — clicking empty vertical space next to a note no
/// longer selects it (which previously broke Move/Eraser on empty space).
/// Full-height regions (loop, markers, full-lane audio) only bind their X
/// span, so a small X tolerance is added for hairline/marker regions.
fn region_at(
    program: &Program,
    transform: &crate::transform::Transform,
    pos_data: crate::geom::Point,
) -> Option<crate::region::DataRegion> {
    let _ = transform;
    let plot = program.plot();
    plot.regions()
        .iter()
        .filter(|r| r.visible && !r.locked)
        .filter(|r| {
            let hit = r.contains(pos_data.x, pos_data.y);
            // Fallback for full-column regions whose value_range() is None:
            // allow the X span to match regardless of Y.
            if !hit && r.value_range().is_none() {
                r.contains(pos_data.x, pos_data.y)
                    || r.time_range()
                        .is_some_and(|t| pos_data.x >= t.min && pos_data.x <= t.max)
            } else {
                hit
            }
        })
        .max_by_key(|r| r.z_index)
        .cloned()
}

/// Region ids currently flagged `selected` in the plot (box-select result).
///
/// Reading the flag from the model (instead of tool state) keeps group edits
/// alive across program rebuilds / track switches.
fn selected_regions(program: &Program) -> Vec<crate::region::RegionId> {
    program
        .plot()
        .regions()
        .iter()
        .filter(|r| r.selected)
        .map(|r| r.id)
        .collect()
}

/// Erase the region under the cursor with a single click.
fn erase_at(
    program: &Program,
    transform: &crate::transform::Transform,
    pos_data: crate::geom::Point,
) {
    // Try region first
    if let Some(region) = region_at(program, transform, pos_data) {
        program.emit_tool_event(ToolEvent::DeleteRegion { region: region.id });
        return;
    }
    // Otherwise ask the host to delete the nearest point in a visible series
    if let Some(series_id) = program
        .plot()
        .series()
        .iter()
        .find(|s| s.is_visible())
        .map(|s| s.id())
    {
        program.emit_tool_event(ToolEvent::DeletePoint { series: series_id, index: usize::MAX });
    }
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