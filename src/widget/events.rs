//! Translation of iced canvas events into plot interactions.
//!
//! Ports the mouse handlers of the GPUI backend view (`on_mouse_down`,
//! `on_mouse_move`, `on_mouse_up`, `on_scroll`, `on_hover_state_change`) on
//! top of the shared interaction helpers (`pan_viewport`, `zoom_viewport`,
//! `zoom_to_rect`, `toggle_pin`, `zoom_factor_from_drag`).
// use crate::widget::plot;
use std::time::Instant;
use iced::mouse;
use iced::widget::canvas::{self, Action, Event};
use iced::{Point, Rectangle};

use crate::geom::{ScreenPoint, ScreenRect};
use crate::interaction::{
    pan_viewport, toggle_pin, zoom_factor_from_drag, zoom_to_rect, zoom_viewport, HitRegion,
};
use crate::view::Viewport;

use super::hover::update_hover_target;
use super::state::{ClickState, DragMode, DragState, LastClick, PlotUiState};
use super::tools::{ToolGesture, ToolState};
use super::{Message, Program, Tool, ToolEvent};

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

    // ---- TOOLS: active tool hijacks left-drag inside the plot area ----
    if button == mouse::Button::Left && region == HitRegion::Plot {
        let tool = program.current_tool();
        if !matches!(tool, Tool::Select) {
            run_tool_mouse(program, state, tool, pos, ToolPhase::Down);
            // Still track a drag for gesture-based tools (Move/Scale/Stretch)
            if matches!(tool, Tool::Move | Tool::Scale | Tool::Stretch | Tool::Line | Tool::Curve) {
                state.drag = Some(DragState::new(DragMode::Pan, pos, true));
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
            // --- DEBUG-PRINT: AUSLESEN DER AKTUELLEN PLOT ID ---
            println!("[DEBUG: MOUSE_MOVE] Maus ist AKTIV über der Signalfläche von: {}", program.plot().id);

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
    let tool_dragging = state.drag.as_ref().is_some_and(|d| d.active);
    let tool = program.current_tool();
    if tool_dragging && !matches!(tool, Tool::Select) && hit_region == HitRegion::Plot {
        run_tool_mouse(program, state, tool, pos, ToolPhase::Move);
        if matches!(tool, Tool::Draw) {
            // Freehand consumes the move; no pan
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
                DragMode::ZoomRect => unreachable!("handled above"),
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
    let tool = program.current_tool();
    if !matches!(tool, Tool::Select) && button == mouse::Button::Left {
        if let Some(pos) = position {
            run_tool_mouse(program, state, tool, pos, ToolPhase::Up);
            state.selection_rect = None;
            state.last_click = None;
            state.last_pin_toggle = None;
            return Some(canvas::Action::request_redraw());
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

    // --- DOUBLE-CLICK DETECTION FOR RESETVIEW ---
    // Check if this is a left-click release (not a drag) and detect double-click.
    let was_click = button == mouse::Button::Left 
        && drag.as_ref().is_none_or(|d| d.mode != DragMode::ZoomRect);

    if was_click {
        if let Some(pos) = position {
            // Check if this is a double-click with the previously stored click
            if let Some(last_click) = state.last_click {
                if is_double_click(&last_click, pos, button) {
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
    // Restrict to the plot area
    if !super::geometry::rect_contains(plot_rect, pos) {
        return;
    }

    let raw_point = transform.screen_to_data(pos).unwrap_or(crate::geom::Point::new(0.0, 0.0));

    // Snap the point using the program's snap engine
    let (snapped_point, did_snap) = program.with_snap(|snap| {
        let regions: Vec<crate::region::DataRegion> = program.plot().regions().to_vec();
        let view_range = program
            .plot()
            .viewport()
            .map(|v| crate::region::Range::new(v.x.min, v.x.max))
            .unwrap_or(crate::region::Range::new(0.0, 1.0));
        let result = snap.snap(raw_point, &regions, &transform, view_range, 120.0);
        (result.point, result.snapped)
    });
    let _ = did_snap;

    let screen = pos;
    program.with_tool_state(|ts| {
        match phase {
            ToolPhase::Down => {
                match tool {
                    Tool::Pen | Tool::Eraser | Tool::Split => ts.start_click(snapped_point, screen),
                    Tool::Move | Tool::Scale | Tool::Stretch | Tool::Line | Tool::Curve => {
                        ts.start_drag(snapped_point, screen)
                    }
                    Tool::Draw => ts.start_draw(snapped_point),
                    _ => {}
                }
            }
            ToolPhase::Move => {
                match ts.gesture_mut() {
                    super::tools::ToolGesture::Drag { current, current_screen, .. } => {
                        *current = snapped_point;
                        *current_screen = screen;
                    }
                    super::tools::ToolGesture::Draw { points } => {
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
    match tool {
        Tool::Pen => {
            if let super::tools::ToolGesture::Click { position, .. } =
                std::mem::replace(&mut ts.gesture, super::tools::ToolGesture::None)
            {
                // Find the first visible series to insert into (host may ignore)
                let series_id = program
                    .plot()
                    .series()
                    .iter()
                    .find(|s| s.is_visible())
                    .map(|s| s.id());
                if let Some(series) = series_id {
                    program.emit_tool_event(ToolEvent::InsertPoint { series, point: position });
                } else {
                    program.emit_tool_event(ToolEvent::Notify(
                        "Pen: no visible series to insert into".into(),
                    ));
                }
            }
        }
        Tool::Eraser => {
            if let super::tools::ToolGesture::Click { position, .. } =
                std::mem::replace(&mut ts.gesture, super::tools::ToolGesture::None)
            {
                erase_at(program, transform, position);
            }
        }
        Tool::Split => {
            if let super::tools::ToolGesture::Click { position, .. } =
                std::mem::replace(&mut ts.gesture, super::tools::ToolGesture::None)
            {
                if let Some(region) = region_at(program, transform, position) {
                    program.emit_tool_event(ToolEvent::SplitRegion {
                        region: region.id,
                        time: position.x,
                    });
                }
            }
        }
        Tool::Select => {
            if let super::tools::ToolGesture::Drag { start, current, .. } =
                std::mem::replace(&mut ts.gesture, super::tools::ToolGesture::None)
            {
                let dx = current.x - start.x;
                let dy = current.y - start.y;
                let _ = (dx, dy);
            }
        }
        Tool::Move => {
            if let super::tools::ToolGesture::Drag { start, current, .. } =
                std::mem::replace(&mut ts.gesture, super::tools::ToolGesture::None)
            {
                program.emit_tool_event(ToolEvent::MoveSelection {
                    dx: current.x - start.x,
                    dy: current.y - start.y,
                });
            }
        }
        Tool::Draw => {
            if let super::tools::ToolGesture::Draw { points } =
                std::mem::replace(&mut ts.gesture, super::tools::ToolGesture::None)
            {
                if let Some(series) = program
                    .plot()
                    .series()
                    .iter()
                    .find(|s| s.is_visible())
                    .map(|s| s.id())
                {
                    for point in points {
                        program.emit_tool_event(ToolEvent::InsertPoint { series, point });
                    }
                }
            }
        }
        Tool::Line | Tool::Curve => {
            if let super::tools::ToolGesture::Drag { start, current, .. } =
                std::mem::replace(&mut ts.gesture, super::tools::ToolGesture::None)
            {
                if let Some(series) = program
                    .plot()
                    .series()
                    .iter()
                    .find(|s| s.is_visible())
                    .map(|s| s.id())
                {
                    // 8 interpolated points between start and end
                    let steps = 8;
                    for i in 0..=steps {
                        let t = i as f64 / steps as f64;
                        let point = crate::geom::Point::new(
                            start.x + (current.x - start.x) * t,
                            start.y + (current.y - start.y) * t,
                        );
                        program.emit_tool_event(ToolEvent::InsertPoint { series, point });
                    }
                }
            }
        }
        _ => {
            ts.gesture = super::tools::ToolGesture::None;
        }
    }
}

/// Find the region under the cursor (topmost).
fn region_at(
    program: &Program,
    transform: &crate::transform::Transform,
    pos_data: crate::geom::Point,
) -> Option<crate::region::DataRegion> {
    let plot = program.plot();
    plot.regions()
        .iter()
        .filter(|r| r.visible && !r.locked)
        .filter(|r| {
            r.time_range()
                .is_some_and(|t| pos_data.x >= t.min && pos_data.x <= t.max)
        })
        .max_by_key(|r| r.z_index)
        .cloned()
}

/// Erase regions or points near the cursor.
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
        let _ = transform;
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