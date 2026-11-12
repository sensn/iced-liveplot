use std::cmp::Ordering;


use iced::{Color, Rectangle};
use iced::widget::canvas::{Fill, Stroke, Path};
use iced::Point as IcedPoint;

use crate::axis::{AxisConfig, AxisLayout, TextMeasurer};
use crate::geom::{Point as DataPoint, ScreenPoint, ScreenRect};
use crate::plot::Plot;
use crate::region::{DataRegion, RegionId, RegionKind, RegionStyle, LabelPosition, SerializableColor};
use crate::render::{
    RenderCommand, LineSegment, LineStyle, MarkerShape, MarkerStyle, RectStyle,
    RenderCacheKey, RenderList, TextStyle, build_line_segments, build_scatter_points,
};
use crate::series::{Series, SeriesKind};
use crate::style::Theme;
use crate::transform::Transform;
use crate::view::{Range, Viewport};

use super::config::PlotViewConfig;
use super::constants::*;
use super::geometry::{
    clamp_point, distance_sq, normalized_rect, rect_intersects, rect_intersects_any,
};
use super::hover::update_hover_target;
use super::state::{LegendEntry, LegendLayout, PlotUiState};
use super::text::EstimatedTextMeasurer;

/// Default interaction/layout configuration used by the iced widget.
pub(crate) static DEFAULT_CONFIG: PlotViewConfig = PlotViewConfig {
    drag_threshold_px: 4.0,
    pin_threshold_px: 12.0,
    unpin_threshold_px: 18.0,
    padding_frac: 0.05,
    min_padding: 1e-6,
    show_legend: true,
    show_hover: true,
};

#[derive(Debug, Clone)]
pub(crate) struct PlotFrame {
    pub(crate) render: RenderList,
}

pub(crate) fn build_frame(
    program: &crate::widget::Program,
    plot: &mut Plot,
    state: &mut PlotUiState,
    config: &PlotViewConfig,
    bounds: Rectangle<f32>,
) -> PlotFrame {
    let mut render = RenderList::new();

    let full_width = bounds.width;
    let full_height = bounds.height;
    if full_width <= 1.0 || full_height <= 1.0 {
        return PlotFrame { render };
    }

    let viewport = plot
        .refresh_viewport(config.padding_frac, config.min_padding)
        .unwrap_or_else(|| Viewport::new(Range::new(0.0, 1.0), Range::new(0.0, 1.0)));

    state.viewport = Some(viewport);

    let measurer = EstimatedTextMeasurer;

    let mut plot_width = full_width;
    let mut plot_height = full_height;

    let x_layout = state
        .x_layout
        .update(plot.x_axis(), viewport.x, plot_width as u32, &measurer)
        .clone();
    let y_layout = state
        .y_layout
        .update(plot.y_axis(), viewport.y, plot_height as u32, &measurer)
        .clone();

    let x_title = axis_title_text(plot.x_axis());
    let x_title_size = x_title
        .as_ref()
        .map(|title| measurer.measure(title, plot.x_axis().label_size()))
        .unwrap_or((0.0, 0.0));

    let x_axis_height =
        x_layout.max_label_size.1 + TICK_LENGTH_MAJOR + AXIS_PADDING * 2.0 + x_title_size.1;
    let y_axis_width = y_layout.max_label_size.0 + TICK_LENGTH_MAJOR + AXIS_PADDING * 2.0;

    let x_axis_height = x_axis_height.clamp(0.0, full_height - 1.0);
    let y_axis_width = y_axis_width.clamp(0.0, full_width - 1.0);

    plot_width = (full_width - y_axis_width).max(1.0);
    plot_height = (full_height - x_axis_height).max(1.0);

    let x_layout = state
        .x_layout
        .update(plot.x_axis(), viewport.x, plot_width as u32, &measurer)
        .clone();
    let y_layout = state
        .y_layout
        .update(plot.y_axis(), viewport.y, plot_height as u32, &measurer)
        .clone();

    let origin_x = bounds.x;
    let origin_y = bounds.y;
    let full_max_x = origin_x + full_width;
    let full_max_y = origin_y + full_height;

    let plot_rect = ScreenRect::new(
        ScreenPoint::new(origin_x + y_axis_width, origin_y),
        ScreenPoint::new(full_max_x, full_max_y - x_axis_height),
    );
    let x_axis_rect = ScreenRect::new(
        ScreenPoint::new(plot_rect.min.x, plot_rect.max.y),
        ScreenPoint::new(plot_rect.max.x, full_max_y),
    );
    let y_axis_rect = ScreenRect::new(
        ScreenPoint::new(origin_x, plot_rect.min.y),
        ScreenPoint::new(plot_rect.min.x, plot_rect.max.y),
    );

    state.regions = crate::interaction::PlotRegions {
        plot: plot_rect,
        x_axis: x_axis_rect,
        y_axis: y_axis_rect,
        pins: std::collections::HashSet::new(),
    };
    state.plot_rect = Some(plot_rect);

    let transform = Transform::new(viewport, plot_rect);
    state.transform = transform.clone();

    if let Some(transform) = transform {
        build_grid(
            &mut render,
            plot,
            &x_layout,
            &y_layout,
            &transform,
            plot_rect,
        );
        build_series(&mut render, plot, state, &transform, plot_rect);
        build_linked_brush(&mut render, plot, state, &transform, plot_rect);
        build_playhead(&mut render, program, state, &transform, plot_rect);
        build_selection(&mut render, plot, state);
        build_regions(&mut render, plot, &transform, plot_rect, &measurer);
        update_hover_target(
            plot,
            state,
            &transform,
            plot_rect,
            config.pin_threshold_px,
            config.unpin_threshold_px,
        );
        build_linked_cursor(&mut render, plot, state, &transform, plot_rect, &measurer);
        build_pins(&mut render, plot, &transform, plot_rect, &measurer);
        build_axes(
            &mut render,
            plot,
            &x_layout,
            &y_layout,
            plot_rect,
            &transform,
            x_axis_rect,
            y_axis_rect,
            &measurer,
        );
        if config.show_hover {
            build_hover(&mut render, plot, state, &transform, plot_rect, &measurer);
        }
        if config.show_legend {
            build_legend(&mut render, plot, state, plot_rect, &measurer);
        } else {
            state.legend_layout = None;
        }
        build_axis_titles(
            &mut render,
            plot,
            plot_rect,
            x_axis_rect,
            y_axis_rect,
            &measurer,
        );
    } else {
        state.legend_layout = None;
        let message = "Invalid axis range";
        let size = measurer.measure(message, 14.0);
        let pos = ScreenPoint::new(
            plot_rect.min.x + (plot_rect.width() - size.0) * 0.5,
            plot_rect.min.y + (plot_rect.height() - size.1) * 0.5,
        );
        render.push(RenderCommand::Text {
            position: pos,
            text: message.to_string(),
            style: TextStyle {
                color: plot.theme().axis,
                size: 14.0,
            },
        });
    }

    PlotFrame { render }
}

fn build_grid(
    render: &mut RenderList,
    plot: &Plot,
    x_layout: &AxisLayout,
    y_layout: &AxisLayout,
    transform: &Transform,
    plot_rect: ScreenRect,
) {
    let theme = plot.theme();
    let mut major = Vec::new();
    let mut minor = Vec::new();

    if plot.x_axis().show_grid() {
        for tick in &x_layout.ticks {
            let x = transform
                .data_to_screen(DataPoint::new(tick.value, transform.viewport().y.min))
                .map(|p| p.x);
            let Some(x) = x else { continue };
            let segment = LineSegment::new(
                ScreenPoint::new(x, plot_rect.min.y),
                ScreenPoint::new(x, plot_rect.max.y),
            );
            if tick.is_major {
                major.push(segment);
            } else if plot.x_axis().show_minor_grid() {
                minor.push(segment);
            }
        }
    }

    if plot.y_axis().show_grid() {
        for tick in &y_layout.ticks {
            let y = transform
                .data_to_screen(DataPoint::new(transform.viewport().x.min, tick.value))
                .map(|p| p.y);
            let Some(y) = y else { continue };
            let segment = LineSegment::new(
                ScreenPoint::new(plot_rect.min.x, y),
                ScreenPoint::new(plot_rect.max.x, y),
            );
            if tick.is_major {
                major.push(segment);
            } else if plot.y_axis().show_minor_grid() {
                minor.push(segment);
            }
        }
    }

    render.push(RenderCommand::ClipRect(plot_rect));
    if !minor.is_empty() {
        render.push(RenderCommand::LineSegments {
            segments: minor,
            style: LineStyle {
                color: theme.grid_minor,
                width: 1.0,
            },
        });
    }
    if !major.is_empty() {
        render.push(RenderCommand::LineSegments {
            segments: major,
            style: LineStyle {
                color: theme.grid_major,
                width: 1.0,
            },
        });
    }

    if plot.x_axis().show_zero_line() {
        if transform.viewport().y.min <= 0.0 && transform.viewport().y.max >= 0.0 {
            if let Some(y) = transform
                .data_to_screen(DataPoint::new(transform.viewport().x.min, 0.0))
                .map(|p| p.y)
            {
                render.push(RenderCommand::LineSegments {
                    segments: vec![LineSegment::new(
                        ScreenPoint::new(plot_rect.min.x, y),
                        ScreenPoint::new(plot_rect.max.x, y),
                    )],
                    style: LineStyle {
                        color: theme.axis,
                        width: 1.0,
                    },
                });
            }
        }
    }

    if plot.y_axis().show_zero_line() {
        if transform.viewport().x.min <= 0.0 && transform.viewport().x.max >= 0.0 {
            if let Some(x) = transform
                .data_to_screen(DataPoint::new(0.0, transform.viewport().y.min))
                .map(|p| p.x)
            {
                render.push(RenderCommand::LineSegments {
                    segments: vec![LineSegment::new(
                        ScreenPoint::new(x, plot_rect.min.y),
                        ScreenPoint::new(x, plot_rect.max.y),
                    )],
                    style: LineStyle {
                        color: theme.axis,
                        width: 1.0,
                    },
                });
            }
        }
    }

    render.push(RenderCommand::ClipEnd);
}

fn build_series(
    render: &mut RenderList,
    plot: &Plot,
    state: &mut PlotUiState,
    transform: &Transform,
    plot_rect: ScreenRect,
) {
    let plot_width = plot_rect.width().max(1.0) as usize;
    let size = (
        plot_rect.width().round() as u32,
        plot_rect.height().round() as u32,
    );

    render.push(RenderCommand::ClipRect(plot_rect));

    for series in plot.series() {
        if !series.is_visible() {
            continue;
        }
        let cache = state.series_cache.entry(series.id()).or_default();
        let key = RenderCacheKey {
            viewport: transform.viewport(),
            size,
            generation: series.generation(),
        };
        if cache.key.as_ref() != Some(&key) {
            series.with_store(|store| {
                let decimated = store.decimate(
                    transform.viewport().x,
                    plot_width,
                    &mut state.decimation_scratch,
                );
                cache.points.clear();
                cache.points.extend_from_slice(decimated);
            });
            cache.key = Some(key.clone());
        }

        match series.kind() {
            SeriesKind::Line(style) => {
                let mut segments = Vec::new();
                build_line_segments(&cache.points, transform, plot_rect, &mut segments);
                if !segments.is_empty() {
                    render.push(RenderCommand::LineSegments {
                        segments,
                        style: *style,
                    });
                }
            }
            SeriesKind::Scatter(style) => {
                let mut points = Vec::new();
                build_scatter_points(&cache.points, transform, plot_rect, &mut points);
                if !points.is_empty() {
                    render.push(RenderCommand::Points {
                        points,
                        style: *style,
                    });
                }
            }
        }
    }

    render.push(RenderCommand::ClipEnd);
}

/// Draw the transport playhead: a bright vertical line with a triangle flag
/// at the top of the plot. Hidden when the playhead is at position 0 and the
/// transport has never been started.
fn build_playhead(
    render: &mut RenderList,
    program: &crate::widget::Program,
    state: &PlotUiState,
    transform: &Transform,
    plot_rect: ScreenRect,
) {
    let _ = state;
    let tick = program.playhead_ticks();
    if tick == 0 {
        return;
    }
    // Use the SAME tempo-aware scale as the playhead scrub (X-axis click) and
    // the tools — otherwise the drawn playhead drifts from where it is set.
    let seconds = tick as f64 / program.ticks_per_second();
    let Some(point) = transform.data_to_screen(crate::geom::Point::new(seconds, 0.0)) else {
        return;
    };
    let x = point.x;
    if x < plot_rect.min.x || x > plot_rect.max.x {
        return;
    }
    // Vertical line
    render.push(RenderCommand::Rect {
        rect: ScreenRect::new(
            ScreenPoint::new(x - 1.0, plot_rect.min.y),
            ScreenPoint::new(x + 1.0, plot_rect.max.y),
        ),
        style: RectStyle {
            fill: Color::from_rgba(1.0, 1.0, 1.0, 0.9),
            stroke: Color::from_rgba(0.0, 0.0, 0.0, 0.4),
            stroke_width: 0.5,
            dashed: false,
        },
    });
    // Triangle flag at the top
    let mut builder = iced::widget::canvas::path::Builder::new();
    builder.move_to(iced::Point::new(x - 5.0, plot_rect.min.y - 8.0));
    builder.line_to(iced::Point::new(x + 5.0, plot_rect.min.y - 8.0));
    builder.line_to(iced::Point::new(x, plot_rect.min.y));
    builder.close();
    let flag = builder.build();
    render.push(RenderCommand::Path {
        path: flag,
        fill: Some(Color::from_rgba(1.0, 1.0, 1.0, 0.9)),
        stroke: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.5)),
        stroke_width: 1.0,
    });
}

fn build_selection(render: &mut RenderList, plot: &Plot, state: &PlotUiState) {
    if let Some(rect) = state.selection_rect {
        let rect = normalized_rect(rect);
        render.push(RenderCommand::Rect {
            rect,
            style: RectStyle {
                // Marching-ants dotted outline for the box-select rubber band
                stroke: plot.theme().selection_border,
                stroke_width: 1.5,
                fill: plot.theme().selection_fill,
                dashed: true,
            },
        });
    }
}

fn build_pins(
    render: &mut RenderList,
    plot: &Plot,
    transform: &Transform,
    plot_rect: ScreenRect,
    measurer: &EstimatedTextMeasurer,
) {
    if plot.pins().is_empty() {
        return;
    }

    let theme = plot.theme();
    // Synchronisiere Typografie-Basics exakt mit build_hover
    let font_size = 11.0; 
    let line_height = 14.0;
    let mut labels: Vec<PinLabel> = Vec::new();
    render.push(RenderCommand::ClipRect(plot_rect));

    for pin in plot.pins() {
        let Some(series) = plot
            .series()
            .iter()
            .find(|series| series.id() == pin.series)
        else {
            continue;
        };
        if !series.is_visible() {
            continue;
        }
        let Some(point) = series.with_store(|store| store.data().point(pin.index)) else {
            continue;
        };
        let Some(screen) = transform.data_to_screen(point) else {
            continue;
        };

        if screen.x < plot_rect.min.x
            || screen.x > plot_rect.max.x
            || screen.y < plot_rect.min.y
            || screen.y > plot_rect.max.y
        {
            continue;
        }

        let (marker_style, base_size) = marker_style_and_size(series);

        let ring_outer = base_size + PIN_RING_OUTER_PAD;
        let ring_inner = base_size + PIN_RING_INNER_PAD;
        render.push(RenderCommand::Points {
            points: vec![screen],
            style: MarkerStyle {
                color: theme.axis,
                size: ring_outer,
                shape: MarkerShape::Circle,
            },
        });
        render.push(RenderCommand::Points {
            points: vec![screen],
            style: MarkerStyle {
                color: theme.background,
                size: ring_inner,
                shape: MarkerShape::Circle,
            },
        });

        render.push(RenderCommand::Points {
            points: vec![screen],
            style: marker_style,
        });

        let x_text = plot.x_axis().format_value(point.x);
        let y_text = plot.y_axis().format_value(point.y);
        
        // Exakt dieselbe 3-zeilige Label-Struktur wie in build_hover!
        let label = format!("{}\nx: {x_text}\ny: {y_text}", series.name());
        
        // Wir überschreiben die gemessene Größe mit der perfekten Festgröße aus build_hover
        let size = (130.0, 48.0);
        
        labels.push(PinLabel {
            screen,
            label,
            size,
        });
    }

    if labels.is_empty() {
        render.push(RenderCommand::ClipEnd);
        return;
    }

    let plot_area = plot_rect.width().max(1.0) * plot_rect.height().max(1.0);
    let total_label_area: f32 = labels.iter().map(|label| label.size.0 * label.size.1).sum();
    let dense =
        labels.len() > MAX_PIN_LABELS || total_label_area > plot_area * MAX_PIN_LABEL_COVERAGE;

    let mut clusters = cluster_pin_labels(&labels, PIN_CLUSTER_RADIUS);
    clusters.sort_by(|a, b| {
        let size_cmp = b.len().cmp(&a.len());
        if size_cmp != Ordering::Equal {
            return size_cmp;
        }
        let min_a = a.iter().copied().min().unwrap_or(0);
        let min_b = b.iter().copied().min().unwrap_or(0);
        min_a.cmp(&min_b)
    });

    let mut placed: Vec<ScreenRect> = Vec::new();
    let mut single_budget = if dense { MAX_PIN_LABELS } else { usize::MAX };
    for cluster in clusters {
        if cluster.len() >= 2 {
            if !dense {
                let mut local_placed = placed.clone();
                let mut placements: Vec<(ScreenPoint, ScreenRect, usize)> = Vec::new();
                let mut success = true;
                for index in &cluster {
                    let entry = &labels[*index];
                    if let Some((origin, rect)) = place_label(
                        entry.screen,
                        entry.size,
                        plot_rect,
                        PIN_LABEL_OFFSET,
                        &local_placed,
                    ) {
                        local_placed.push(rect);
                        placements.push((origin, rect, *index));
                    } else {
                        success = false;
                        break;
                    }
                }

                if success {
                    placed = local_placed;
                    for (origin, rect, index) in placements {
                        let entry = &labels[index];
                        push_label_with_leader(
                            render,
                            rect,
                            origin,
                            entry.screen,
                            &entry.label,
                            font_size,
                            line_height,
                            theme,
                        );
                    }
                    continue;
                }
            }

            let center = cluster_center(&labels, &cluster);
            let label = format!("{} pins", cluster.len());
            let size = (65.0, 20.0); // Kompakte Cluster-Box
            
            if let Some((origin, rect)) =
                place_label(center, size, plot_rect, PIN_LABEL_OFFSET, &placed)
            {
                placed.push(rect);
                push_label_with_leader(
                    render,
                    rect,
                    origin,
                    center,
                    &label,
                    font_size,
                    line_height,
                    theme,
                );
            }
            continue;
        }

        if single_budget == 0 {
            continue;
        }
        let index = cluster[0];
        let entry = &labels[index];

        // --- DER FIX: EXAKT SYNCHRON ZU BUILD_HOVER ---
        // Übergibt die perfekte Geometrie (130.0, 48.0) an das Platzierungssystem
        // und reicht font_size (11.0) und line_height (14.0) sauber an den Painter weiter.
        if let Some((origin, rect)) = place_label(
            entry.screen,
            entry.size, 
            plot_rect,
            PIN_LABEL_OFFSET,
            &placed,
        ) {
            placed.push(rect);
            push_label_with_leader(
                render,
                rect,
                origin,
                entry.screen,
                &entry.label,
                font_size,
                line_height, 
                theme,
            );
            single_budget = single_budget.saturating_sub(1);
        }
    }

    render.push(RenderCommand::ClipEnd);
}

// ========================================================================
// Region rendering
// ========================================================================

fn build_regions(
    render: &mut RenderList,
    plot: &Plot,
    transform: &Transform,
    plot_rect: ScreenRect,
    measurer: &EstimatedTextMeasurer,
) {
    let theme = plot.theme();
    
    // Check if plot has regions
    if plot.regions().is_empty() {
        return;
    }
    
    render.push(RenderCommand::ClipRect(plot_rect));
    
    for region in plot.regions() {
        if !region.visible {
            continue;
        }
        
        // Get screen bounds for this region
        let Some(screen_bounds) = region.screen_bounds(transform) else {
            continue;
        };
        
        // Clip to plot rect
        let mut min_x = screen_bounds.min.x.max(plot_rect.min.x);
        let mut max_x = screen_bounds.max.x.min(plot_rect.max.x);
        let min_y = screen_bounds.min.y.max(plot_rect.min.y);
        let max_y = screen_bounds.max.y.min(plot_rect.max.y);
        
        // Point-like regions (markers, automation points) collapse to zero
        // width; give them a minimal 2px on-screen footprint so they stay
        // visible and hittable.
        let is_point_like = matches!(
            region.kind,
            crate::region::RegionKind::Marker { .. } | crate::region::RegionKind::AutomationPoint { .. }
        );
        if min_x >= max_x {
            if !is_point_like {
                continue;
            }
            let clamped = ((min_x + max_x) * 0.5).max(plot_rect.min.x).min(plot_rect.max.x);
            min_x = (clamped - 1.0).max(plot_rect.min.x);
            max_x = (clamped + 1.0).min(plot_rect.max.x);
        }
        if min_x >= max_x || min_y >= max_y {
            continue;
        }
        
        let clipped_bounds = ScreenRect::new(
            ScreenPoint::new(min_x, min_y),
            ScreenPoint::new(max_x, max_y),
        );
        draw_region_body(render, region, clipped_bounds, plot_rect, theme);
    }
    
    render.push(RenderCommand::ClipEnd);
}
#[allow(clippy::too_many_arguments)]
fn draw_region_body(
    render: &mut RenderList,
    region: &crate::region::DataRegion,
    clipped_bounds: ScreenRect,
    plot_rect: ScreenRect,
    theme: &crate::style::Theme,
) {
    let plot_top = plot_rect.min.y;
    let plot_bottom = plot_rect.max.y;
    let plot_right = plot_rect.max.x;
    let plot_left = plot_rect.min.x;
    let _ = plot_left;
    
    // Get region style
    let fill_color = region.style.fill.unwrap_or(match &region.kind {
        crate::region::RegionKind::MidiNote { .. } => SerializableColor::new(0.4, 0.6, 1.0, 0.7),
        crate::region::RegionKind::AudioRegion { .. } => SerializableColor::new(0.6, 0.8, 0.4, 0.6),
        crate::region::RegionKind::AutomationPoint { .. } => SerializableColor::new(1.0, 0.6, 0.2, 0.9),
        crate::region::RegionKind::Marker { color, .. } => *color,
        crate::region::RegionKind::Loop { .. } => SerializableColor::new(1.0, 0.8, 0.0, 0.3),
        _ => SerializableColor::new(0.5, 0.5, 0.5, 0.5),
    }).to_iced();
    
    let stroke_color = region.style.stroke.unwrap_or_else(|| SerializableColor::from_iced(theme.selection_border)).to_iced();
    let stroke_width = region.style.stroke_width;
    
    // Draw region based on kind
    match &region.kind {
            crate::region::RegionKind::MidiNote { pitch, velocity, channel, .. } => {
                // MIDI note as rectangle
                render.push(RenderCommand::Rect {
                    rect: clipped_bounds,
                    style: RectStyle {
                        fill: fill_color,
                        stroke: stroke_color,
                        stroke_width,
                        dashed: false,
                    },
                });
                
                // Velocity indicator (top bar)
                let vel_height = (clipped_bounds.height() * *velocity as f32 / 127.0).max(1.0);
                let vel_rect = ScreenRect::new(
                    ScreenPoint::new(clipped_bounds.min.x, clipped_bounds.max.y - vel_height),
                    ScreenPoint::new(clipped_bounds.max.x, clipped_bounds.max.y),
                );
                render.push(RenderCommand::Rect {
                    rect: vel_rect,
                    style: RectStyle {
                        fill: Color::WHITE,
                        stroke: Color::TRANSPARENT,
                        stroke_width: 0.0,
                        dashed: false,
                    },
                });
                
                // Channel indicator (small colored bar)
                if *channel > 0 {
                    let chan_colors = [
                        SerializableColor::new(1.0, 0.0, 0.0, 1.0),
                        SerializableColor::new(1.0, 0.5, 0.0, 1.0),
                        SerializableColor::new(1.0, 1.0, 0.0, 1.0),
                        SerializableColor::new(0.0, 1.0, 0.0, 1.0),
                        SerializableColor::new(0.0, 1.0, 1.0, 1.0),
                        SerializableColor::new(0.0, 0.5, 1.0, 1.0),
                        SerializableColor::new(0.0, 0.0, 1.0, 1.0),
                        SerializableColor::new(0.5, 0.0, 1.0, 1.0),
                        SerializableColor::new(1.0, 0.0, 1.0, 1.0),
                        SerializableColor::new(1.0, 0.0, 0.5, 1.0),
                        SerializableColor::new(0.5, 0.5, 0.5, 1.0),
                        SerializableColor::new(1.0, 1.0, 1.0, 1.0),
                        SerializableColor::new(0.0, 0.0, 0.0, 1.0),
                        SerializableColor::new(1.0, 0.5, 0.5, 1.0),
                        SerializableColor::new(0.5, 1.0, 0.5, 1.0),
                        SerializableColor::new(0.5, 0.5, 1.0, 1.0),
                    ];
                    let chan_color = chan_colors[*channel as usize % chan_colors.len()];
                    let chan_rect = ScreenRect::new(
                        ScreenPoint::new(clipped_bounds.min.x, clipped_bounds.min.y),
                        ScreenPoint::new(clipped_bounds.min.x + 3.0, clipped_bounds.max.y),
                    );
                    render.push(RenderCommand::Rect {
                        rect: chan_rect,
                        style: RectStyle {
                            fill: chan_color.to_iced(),
                            stroke: Color::TRANSPARENT,
                            stroke_width: 0.0,
                            dashed: false,
                        },
                    });
                }
            }
            crate::region::RegionKind::AudioRegion { fade_in, fade_out, .. } => {
                // Audio region as rectangle with fade handles
                render.push(RenderCommand::Rect {
                    rect: clipped_bounds,
                    style: RectStyle {
                        fill: fill_color,
                        stroke: stroke_color,
                        stroke_width,
                        dashed: false,
                    },
                });
                
                // Draw fade in/out indicators
                let region_width = clipped_bounds.width();
                if *fade_in > 0.0 {
                    let fade_width = (region_width * *fade_in as f32).min(region_width / 2.0);
                    let fade_rect = ScreenRect::new(
                        ScreenPoint::new(clipped_bounds.min.x, clipped_bounds.min.y),
                        ScreenPoint::new(clipped_bounds.min.x + fade_width, clipped_bounds.max.y),
                    );
                    render.push(RenderCommand::Rect {
                        rect: fade_rect,
                        style: RectStyle {
                            fill: Color::from_rgba(1.0, 1.0, 1.0, 0.2),
                            stroke: Color::TRANSPARENT,
                            stroke_width: 0.0,
                            dashed: false,
                        },
                    });
                }
                if *fade_out > 0.0 {
                    let fade_width = (region_width * *fade_out as f32).min(region_width / 2.0);
                    let fade_rect = ScreenRect::new(
                        ScreenPoint::new(clipped_bounds.max.x - fade_width, clipped_bounds.min.y),
                        ScreenPoint::new(clipped_bounds.max.x, clipped_bounds.max.y),
                    );
                    render.push(RenderCommand::Rect {
                        rect: fade_rect,
                        style: RectStyle {
                            fill: Color::from_rgba(1.0, 1.0, 1.0, 0.2),
                            stroke: Color::TRANSPARENT,
                            stroke_width: 0.0,
                            dashed: false,
                        },
                    });
                }
            }
            crate::region::RegionKind::AutomationPoint { parameter, value, .. } => {
                // Automation point as diamond/circle
                let center_x = (clipped_bounds.min.x + clipped_bounds.max.x) / 2.0;
                let center_y = (clipped_bounds.min.y + clipped_bounds.max.y) / 2.0;
                let radius = 6.0;
                
                // Draw diamond shape for automation points
                let mut builder = iced::widget::canvas::path::Builder::new();
                builder.move_to(iced::Point::new(center_x, center_y - radius));
                builder.line_to(iced::Point::new(center_x + radius, center_y));
                builder.line_to(iced::Point::new(center_x, center_y + radius));
                builder.line_to(iced::Point::new(center_x - radius, center_y));
                builder.close();
                let diamond = builder.build();
                render.push(RenderCommand::Path {
                    path: diamond,
                    fill: Some(fill_color),
                    stroke: Some(stroke_color),
                    stroke_width,
                });
            }
            crate::region::RegionKind::Marker { label, color, .. } => {
                // Marker: vertical line through the whole plot height + flag
                let line_x = (clipped_bounds.min.x + clipped_bounds.max.x) * 0.5;
                let marker_color = color.to_iced();
                let flag_width = 80.0_f32.min(plot_right - line_x).max(12.0);
                let flag_height = 18.0;
                let _ = label;

                // Vertical line (use the clipped strip extended to plot height)
                let line_rect = ScreenRect::new(
                    ScreenPoint::new(line_x - 1.0, plot_top),
                    ScreenPoint::new(line_x + 1.0, plot_bottom),
                );
                render.push(RenderCommand::Rect {
                    rect: line_rect,
                    style: RectStyle {
                        fill: marker_color,
                        stroke: Color::TRANSPARENT,
                        stroke_width: 0.0,
                        dashed: false,
                    },
                });

                // Triangle flag at the top
                let mut builder = iced::widget::canvas::path::Builder::new();
                builder.move_to(iced::Point::new(line_x, plot_top + flag_height));
                builder.line_to(iced::Point::new(line_x + flag_width, plot_top + flag_height / 2.0));
                builder.line_to(iced::Point::new(line_x, plot_top));
                builder.close();
                let flag = builder.build();
                render.push(RenderCommand::Path {
                    path: flag,
                    fill: Some(marker_color),
                    stroke: Some(Color::WHITE),
                    stroke_width: 1.0,
                });
                // Draw label
                if let Some(label_text) = &region.style.label {
                    render.push(RenderCommand::Text {
                        position: ScreenPoint::new(line_x + 6.0, plot_top + 3.0),
                        text: label_text.clone(),
                        style: TextStyle {
                            color: region.style.label_color.unwrap_or(SerializableColor::WHITE).to_iced(),
                            size: 10.0,
                        },
                    });
                }
            }
            crate::region::RegionKind::Loop { .. } => {
                // Loop as bracket with label
                render.push(RenderCommand::Rect {
                    rect: clipped_bounds,
                    style: RectStyle {
                        fill: fill_color,
                        stroke: stroke_color,
                        stroke_width: 2.0,
                        dashed: false,
                    },
                });
                
                // Draw bracket ends
                let bracket_size = 8.0;
                let left_bracket = ScreenRect::new(
                    ScreenPoint::new(clipped_bounds.min.x, clipped_bounds.min.y),
                    ScreenPoint::new(clipped_bounds.min.x + bracket_size, clipped_bounds.max.y),
                );
                let right_bracket = ScreenRect::new(
                    ScreenPoint::new(clipped_bounds.max.x - bracket_size, clipped_bounds.min.y),
                    ScreenPoint::new(clipped_bounds.max.x, clipped_bounds.max.y),
                );
                render.push(RenderCommand::Rect {
                    rect: left_bracket,
                    style: RectStyle {
                        fill: stroke_color,
                        stroke: Color::TRANSPARENT,
                        stroke_width: 0.0,
                        dashed: false,
                    },
                });
                render.push(RenderCommand::Rect {
                    rect: right_bracket,
                    style: RectStyle {
                        fill: stroke_color,
                        stroke: Color::TRANSPARENT,
                        stroke_width: 0.0,
                        dashed: false,
                    },
                });
            }
            _ => {
                // Generic rectangle for other region types
                render.push(RenderCommand::Rect {
                    rect: clipped_bounds,
                    style: RectStyle {
                        fill: fill_color,
                        stroke: stroke_color,
                        stroke_width,
                        dashed: false,
                    },
                });
            }
        }
        // Resize handles on start/end edges for span regions (MIDI notes,
        // audio clips, loop). Logic Pro style: the strips span the FULL height
        // of the event and extend a few pixels outside it, so they are easy to
        // grab; they are only drawn once the event is selected (which is when
        // trimming is possible).
        let has_edges = matches!(
            region.kind,
            crate::region::RegionKind::MidiNote { .. }
                | crate::region::RegionKind::AudioRegion { .. }
                | crate::region::RegionKind::Loop { .. }
        );
        if has_edges && region.selected {
            let top = clipped_bounds.min.y;
            let bottom = clipped_bounds.max.y;
            // Wider than before (8px + 2px outside) and full event height.
            let handle_w = 6.0;
            let margin = 2.0;
            let handle_color = SerializableColor::new(1.0, 0.85, 0.25, 1.0).to_iced();
            // Start handle
            let start_handle = ScreenRect::new(
                ScreenPoint::new(clipped_bounds.min.x - margin, top),
                ScreenPoint::new(clipped_bounds.min.x + handle_w, bottom),
            );
            render.push(RenderCommand::Rect {
                rect: start_handle,
                style: RectStyle {
                    fill: handle_color,
                    stroke: Color::BLACK,
                    stroke_width: 1.0,
                    dashed: false,
                },
            });
            // End handle
            let end_handle = ScreenRect::new(
                ScreenPoint::new(clipped_bounds.max.x - handle_w, top),
                ScreenPoint::new(clipped_bounds.max.x + margin, bottom),
            );
            render.push(RenderCommand::Rect {
                rect: end_handle,
                style: RectStyle {
                    fill: handle_color,
                    stroke: Color::BLACK,
                    stroke_width: 1.0,
                    dashed: false,
                },
            });
        }
        
        // Draw selection highlight
        if region.selected {
            let highlight_bounds = ScreenRect::new(
                ScreenPoint::new(clipped_bounds.min.x - 2.0, clipped_bounds.min.y - 2.0),
                ScreenPoint::new(clipped_bounds.max.x + 2.0, clipped_bounds.max.y + 2.0),
            );
            render.push(RenderCommand::Rect {
                rect: highlight_bounds,
                style: RectStyle {
                    fill: Color::TRANSPARENT,
                    stroke: SerializableColor::new(1.0, 0.8, 0.2, 1.0).to_iced(),
                    stroke_width: 2.0,
                    dashed: false,
                },
            });
            
            // Draw selection handles
            let handle_size = 6.0;
            let handles = [
                // Corners
                (clipped_bounds.min.x, clipped_bounds.min.y), // TL
                (clipped_bounds.max.x, clipped_bounds.min.y), // TR
                (clipped_bounds.min.x, clipped_bounds.max.y), // BL
                (clipped_bounds.max.x, clipped_bounds.max.y), // BR
                // Edges
                ((clipped_bounds.min.x + clipped_bounds.max.x) / 2.0, clipped_bounds.min.y), // Top
                ((clipped_bounds.min.x + clipped_bounds.max.x) / 2.0, clipped_bounds.max.y), // Bottom
                (clipped_bounds.min.x, (clipped_bounds.min.y + clipped_bounds.max.y) / 2.0), // Left
                (clipped_bounds.max.x, (clipped_bounds.min.y + clipped_bounds.max.y) / 2.0), // Right
            ];
            
            for (hx, hy) in handles {
                let handle_rect = ScreenRect::new(
                    ScreenPoint::new(hx - handle_size / 2.0, hy - handle_size / 2.0),
                    ScreenPoint::new(hx + handle_size / 2.0, hy + handle_size / 2.0),
                );
                render.push(RenderCommand::Rect {
                    rect: handle_rect,
                    style: RectStyle {
                        fill: Color::WHITE,
                        stroke: SerializableColor::new(1.0, 0.6, 0.0, 1.0).to_iced(),
                        stroke_width: 1.5,
                        dashed: false,
                    },
                });
            }
        }
        
        // Draw region label
        if let Some(label_text) = &region.style.label {
            let label_pos = match region.style.label_position {
                LabelPosition::TopLeft => ScreenPoint::new(clipped_bounds.min.x + 4.0, clipped_bounds.min.y + 2.0),
                LabelPosition::TopCenter => ScreenPoint::new(
                    (clipped_bounds.min.x + clipped_bounds.max.x) / 2.0 - 20.0,
                    clipped_bounds.min.y + 2.0
                ),
                LabelPosition::TopRight => ScreenPoint::new(clipped_bounds.max.x - 40.0, clipped_bounds.min.y + 2.0),
                LabelPosition::Center => ScreenPoint::new(
                    (clipped_bounds.min.x + clipped_bounds.max.x) / 2.0 - 20.0,
                    (clipped_bounds.min.y + clipped_bounds.max.y) / 2.0 - 7.0
                ),
                LabelPosition::BottomLeft => ScreenPoint::new(clipped_bounds.min.x + 4.0, clipped_bounds.max.y - 16.0),
                LabelPosition::BottomCenter => ScreenPoint::new(
                    (clipped_bounds.min.x + clipped_bounds.max.x) / 2.0 - 20.0,
                    clipped_bounds.max.y - 16.0
                ),
                LabelPosition::BottomRight => ScreenPoint::new(clipped_bounds.max.x - 40.0, clipped_bounds.max.y - 16.0),
            };
            
            let label_color = region.style.label_color.unwrap_or_else(|| SerializableColor::from_iced(theme.axis));
            render.push(RenderCommand::Text {
                position: label_pos,
                text: label_text.clone(),
                style: TextStyle {
                    color: label_color.to_iced(),
                    size: 10.0,
                },
            });
        }
    
    render.push(RenderCommand::ClipEnd);
}

#[allow(clippy::too_many_arguments)]
fn build_axes(
    render: &mut RenderList,
    plot: &Plot,
    x_layout: &AxisLayout,
    y_layout: &AxisLayout,
    plot_rect: ScreenRect,
    transform: &Transform,
    x_axis_rect: ScreenRect,
    y_axis_rect: ScreenRect,
    measurer: &EstimatedTextMeasurer,
) {
    let theme = plot.theme();
    let mut x_ticks_major = Vec::new();
    let mut x_ticks_minor = Vec::new();
    let mut y_ticks_major = Vec::new();
    let mut y_ticks_minor = Vec::new();
    let label_gap = 2.0_f32;
    let mut last_x_label_right = f32::NEG_INFINITY;
    let mut last_y_label_top = f32::INFINITY;
    let x_title_rect = axis_title_text(plot.x_axis()).map(|title| {
        let size = measurer.measure(&title, plot.x_axis().label_size());
        let pos = clamp_label_position(
            ScreenPoint::new(
                plot_rect.min.x + (plot_rect.width() - size.0) * 0.5,
                x_axis_rect.max.y - size.1 - AXIS_PADDING,
            ),
            size,
            x_axis_rect,
        );
        ScreenRect::new(pos, ScreenPoint::new(pos.x + size.0, pos.y + size.1))
    });
    let y_title_rect = axis_title_text(plot.y_axis()).map(|title| {
        let size = measurer.measure(&title, plot.y_axis().label_size());
        let pos = clamp_label_position(
            ScreenPoint::new(
                y_axis_rect.min.x + AXIS_PADDING,
                y_axis_rect.min.y + AXIS_PADDING,
            ),
            size,
            y_axis_rect,
        );
        ScreenRect::new(pos, ScreenPoint::new(pos.x + size.0, pos.y + size.1))
    });

    if plot.x_axis().show_border() {
        render.push(RenderCommand::Rect {
            rect: plot_rect,
            style: RectStyle {
                fill: Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.0,
                },
                stroke: theme.axis,
                stroke_width: 1.0,
                dashed: false,
            },
        });
    }

    render.push(RenderCommand::ClipRect(x_axis_rect));
    for tick in &x_layout.ticks {
        if let Some(x) = transform
            .data_to_screen(DataPoint::new(tick.value, transform.viewport().y.min))
            .map(|p| p.x)
        {
            let length = if tick.is_major {
                TICK_LENGTH_MAJOR
            } else {
                TICK_LENGTH_MINOR
            };
            let segment = LineSegment::new(
                ScreenPoint::new(x, plot_rect.max.y),
                ScreenPoint::new(x, plot_rect.max.y + length),
            );
            if tick.is_major {
                x_ticks_major.push(segment);
            } else if plot.x_axis().show_minor_grid() {
                x_ticks_minor.push(segment);
            }

            if tick.is_major && !tick.label.is_empty() {
                let size = measurer.measure(&tick.label, plot.x_axis().label_size());
                let pos = x_tick_label_position(x, size, plot_rect);
                let label_rect =
                    ScreenRect::new(pos, ScreenPoint::new(pos.x + size.0, pos.y + size.1));
                if let Some(visible_rect) = tick_label_visible_rect(label_rect, x_axis_rect) {
                    let overlaps_title = x_title_rect
                        .map(|rect| rect_intersects(visible_rect, rect))
                        .unwrap_or(false);
                    if !overlaps_title && visible_rect.min.x >= last_x_label_right + label_gap {
                        render.push(RenderCommand::Text {
                            position: pos,
                            text: tick.label.clone(),
                            style: TextStyle {
                                color: theme.axis,
                                size: plot.x_axis().label_size(),
                            },
                        });
                        last_x_label_right = visible_rect.max.x;
                    }
                }
            }
        }
    }
    if !x_ticks_minor.is_empty() {
        render.push(RenderCommand::LineSegments {
            segments: x_ticks_minor,
            style: LineStyle {
                color: theme.axis,
                width: 1.0,
            },
        });
    }
    if !x_ticks_major.is_empty() {
        render.push(RenderCommand::LineSegments {
            segments: x_ticks_major,
            style: LineStyle {
                color: theme.axis,
                width: 1.0,
            },
        });
    }
    render.push(RenderCommand::ClipEnd);

    render.push(RenderCommand::ClipRect(y_axis_rect));
    for tick in &y_layout.ticks {
        if let Some(y) = transform
            .data_to_screen(DataPoint::new(transform.viewport().x.min, tick.value))
            .map(|p| p.y)
        {
            let length = if tick.is_major {
                TICK_LENGTH_MAJOR
            } else {
                TICK_LENGTH_MINOR
            };
            let segment = LineSegment::new(
                ScreenPoint::new(plot_rect.min.x - length, y),
                ScreenPoint::new(plot_rect.min.x, y),
            );
            if tick.is_major {
                y_ticks_major.push(segment);
            } else if plot.y_axis().show_minor_grid() {
                y_ticks_minor.push(segment);
            }

            if tick.is_major && !tick.label.is_empty() {
                let size = measurer.measure(&tick.label, plot.y_axis().label_size());
                let pos = y_tick_label_position(y, size, plot_rect);
                let label_rect =
                    ScreenRect::new(pos, ScreenPoint::new(pos.x + size.0, pos.y + size.1));
                if let Some(visible_rect) = tick_label_visible_rect(label_rect, y_axis_rect) {
                    let overlaps_title = y_title_rect
                        .map(|rect| rect_intersects(visible_rect, rect))
                        .unwrap_or(false);
                    if !overlaps_title && visible_rect.max.y <= last_y_label_top - label_gap {
                        render.push(RenderCommand::Text {
                            position: pos,
                            text: tick.label.clone(),
                            style: TextStyle {
                                color: theme.axis,
                                size: plot.y_axis().label_size(),
                            },
                        });
                        last_y_label_top = visible_rect.min.y;
                    }
                }
            }
        }
    }
    if !y_ticks_minor.is_empty() {
        render.push(RenderCommand::LineSegments {
            segments: y_ticks_minor,
            style: LineStyle {
                color: theme.axis,
                width: 1.0,
            },
        });
    }
    if !y_ticks_major.is_empty() {
        render.push(RenderCommand::LineSegments {
            segments: y_ticks_major,
            style: LineStyle {
                color: theme.axis,
                width: 1.0,
            },
        });
    }
    render.push(RenderCommand::ClipEnd);
}

fn build_axis_titles(
    render: &mut RenderList,
    plot: &Plot,
    plot_rect: ScreenRect,
    x_axis_rect: ScreenRect,
    y_axis_rect: ScreenRect,
    measurer: &EstimatedTextMeasurer,
) {
    let theme = plot.theme();
    if let Some(title) = axis_title_text(plot.x_axis()) {
        let size = measurer.measure(&title, plot.x_axis().label_size());
        let pos = clamp_label_position(
            ScreenPoint::new(
                plot_rect.min.x + (plot_rect.width() - size.0) * 0.5,
                x_axis_rect.max.y - size.1 - AXIS_PADDING,
            ),
            size,
            x_axis_rect,
        );
        render.push(RenderCommand::Text {
            position: pos,
            text: title,
            style: TextStyle {
                color: theme.axis,
                size: plot.x_axis().label_size(),
            },
        });
    }

    if let Some(title) = axis_title_text(plot.y_axis()) {
        let pos = clamp_label_position(
            ScreenPoint::new(
                y_axis_rect.min.x + AXIS_PADDING,
                y_axis_rect.min.y + AXIS_PADDING,
            ),
            measurer.measure(&title, plot.y_axis().label_size()),
            y_axis_rect,
        );
        render.push(RenderCommand::Text {
            position: pos,
            text: title,
            style: TextStyle {
                color: theme.axis,
                size: plot.y_axis().label_size(),
            },
        });
    }
}

fn clamp_label_position(pos: ScreenPoint, size: (f32, f32), rect: ScreenRect) -> ScreenPoint {
    let max_x = (rect.max.x - size.0).max(rect.min.x);
    let max_y = (rect.max.y - size.1).max(rect.min.y);
    ScreenPoint::new(
        pos.x.clamp(rect.min.x, max_x),
        pos.y.clamp(rect.min.y, max_y),
    )
}

fn x_tick_label_position(x: f32, size: (f32, f32), plot_rect: ScreenRect) -> ScreenPoint {
    ScreenPoint::new(
        x - size.0 * 0.5,
        plot_rect.max.y + TICK_LENGTH_MAJOR + AXIS_PADDING,
    )
}

fn y_tick_label_position(y: f32, size: (f32, f32), plot_rect: ScreenRect) -> ScreenPoint {
    ScreenPoint::new(
        plot_rect.min.x - TICK_LENGTH_MAJOR - AXIS_PADDING - size.0,
        y - size.1 * 0.5,
    )
}

fn tick_label_visible_rect(label_rect: ScreenRect, axis_rect: ScreenRect) -> Option<ScreenRect> {
    let min_x = label_rect.min.x.max(axis_rect.min.x);
    let min_y = label_rect.min.y.max(axis_rect.min.y);
    let max_x = label_rect.max.x.min(axis_rect.max.x);
    let max_y = label_rect.max.y.min(axis_rect.max.y);

    (min_x < max_x && min_y < max_y).then(|| {
        ScreenRect::new(
            ScreenPoint::new(min_x, min_y),
            ScreenPoint::new(max_x, max_y),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x_tick_label_position_keeps_tick_center_anchor() {
        let plot_rect = ScreenRect::new(
            ScreenPoint::new(100.0, 20.0),
            ScreenPoint::new(300.0, 180.0),
        );
        let size = (40.0, 12.0);

        let pos = x_tick_label_position(92.0, size, plot_rect);

        assert_eq!(pos.x, 72.0);
        assert_eq!(pos.y, 192.0);
    }

    #[test]
    fn y_tick_label_position_keeps_tick_center_anchor() {
        let plot_rect = ScreenRect::new(
            ScreenPoint::new(100.0, 20.0),
            ScreenPoint::new(300.0, 180.0),
        );
        let size = (30.0, 10.0);

        let pos = y_tick_label_position(16.0, size, plot_rect);

        assert_eq!(pos.x, 58.0);
        assert_eq!(pos.y, 11.0);
    }

    #[test]
    fn x_tick_label_visible_rect_clips_to_x_axis_bounds() {
        let axis_rect = ScreenRect::new(
            ScreenPoint::new(100.0, 180.0),
            ScreenPoint::new(300.0, 220.0),
        );
        let label_rect = ScreenRect::new(
            ScreenPoint::new(80.0, 192.0),
            ScreenPoint::new(140.0, 204.0),
        );

        let visible = tick_label_visible_rect(label_rect, axis_rect).expect("visible rect");

        assert_eq!(visible.min.x, 100.0);
        assert_eq!(visible.max.x, 140.0);
        assert_eq!(visible.min.y, 192.0);
        assert_eq!(visible.max.y, 204.0);
    }

    #[test]
    fn tick_label_visible_rect_returns_none_when_fully_outside() {
        let axis_rect = ScreenRect::new(
            ScreenPoint::new(100.0, 180.0),
            ScreenPoint::new(300.0, 220.0),
        );
        let label_rect =
            ScreenRect::new(ScreenPoint::new(20.0, 192.0), ScreenPoint::new(60.0, 204.0));

        assert!(tick_label_visible_rect(label_rect, axis_rect).is_none());
    }
}

fn build_hover(
    render: &mut RenderList,
    plot: &Plot,
    state: &PlotUiState,
    transform: &Transform,
    plot_rect: ScreenRect,
    measurer: &EstimatedTextMeasurer,
) {
    let theme = plot.theme();
    let Some(cursor) = state.hover else { return };
    if cursor.x < plot_rect.min.x
        || cursor.x > plot_rect.max.x
        || cursor.y < plot_rect.min.y
        || cursor.y > plot_rect.max.y
    {
        return;
    }

    if let Some(target) = state.hover_target {
        let Some(series) = plot
            .series()
            .iter()
            .find(|series| series.id() == target.pin.series)
        else {
            return;
        };
        let Some(point) = series.with_store(|store| store.data().point(target.pin.index))
        else {
            return;
        };
        let screen = target.screen;
        if screen.x < plot_rect.min.x
            || screen.x > plot_rect.max.x
            || screen.y < plot_rect.min.y
            || screen.y > plot_rect.max.y
        {
            return;
        }

        if target.is_pinned {
            let (_, base_size) = marker_style_and_size(series);
            let ring_outer = base_size + PIN_RING_OUTER_PAD;
            let ring_inner = base_size + PIN_RING_INNER_PAD;
            render.push(RenderCommand::Points {
                points: vec![screen],
                style: MarkerStyle {
                    color: PIN_UNPIN_HIGHLIGHT,
                    size: ring_outer,
                    shape: MarkerShape::Circle,
                },
            });
            render.push(RenderCommand::Points {
                points: vec![screen],
                style: MarkerStyle {
                    color: theme.background,
                    size: ring_inner,
                    shape: MarkerShape::Circle,
                },
            });
            return;
        }

        let (marker_style, base_size) = marker_style_and_size(series);
        let ring_outer = base_size + PIN_RING_OUTER_PAD;
        let ring_inner = base_size + PIN_RING_INNER_PAD;
        render.push(RenderCommand::Points {
            points: vec![screen],
            style: MarkerStyle {
                color: theme.axis,
                size: ring_outer,
                shape: MarkerShape::Circle,
            },
        });
        render.push(RenderCommand::Points {
            points: vec![screen],
            style: MarkerStyle {
                color: theme.background,
                size: ring_inner,
                shape: MarkerShape::Circle,
            },
        });
        render.push(RenderCommand::Points {
            points: vec![screen],
            style: marker_style,
        });

         let x_text = plot.x_axis().format_value(point.x);
        let y_text = plot.y_axis().format_value(point.y);
        let label = format!("{}\nx: {x_text}\ny: {y_text}", series.name());
        
        // --- GPUI-PORT FIX 1: HOVER ÜBER SIGNAL (3 Zeilen) ---
        // Statt measurer.measure nutzen wir eine feste, perfekt abgestimmte Box-Größe für 3 Zeilen Text
        let size = (130.0, 48.0); 
        
        let mut origin = ScreenPoint::new(screen.x + 12.0, screen.y + 12.0);
        if origin.x + size.0 > plot_rect.max.x {
            origin.x = screen.x - size.0 - 12.0;
        }
        if origin.y + size.1 > plot_rect.max.y {
            origin.y = screen.y - size.1 - 12.0;
        }
        origin = clamp_point(origin, plot_rect, size);

        render.push(RenderCommand::Rect {
            rect: ScreenRect::new(
                origin,
                ScreenPoint::new(origin.x + size.0, origin.y + size.1),
            ),
            style: RectStyle {
                fill: theme.pin_bg,
                stroke: theme.pin_border,
                stroke_width: 1.0,
                dashed: false,
            },
        });

        for (index, line) in label.lines().enumerate() {
            let line_y = origin.y + index as f32 * 14.0 + 4.0; // Leicht erhöhtes Padding oben
            render.push(RenderCommand::Text {
                position: ScreenPoint::new(origin.x + 6.0, line_y), // Mehr Innenabstand links
                text: line.to_string(),
                style: TextStyle {
                    color: theme.axis,
                    size: 11.0, // Etwas kompakterer Text für den GPUI-Look
                },
            });
        }
        return;
    }

    // --- FREIER RAUM HOVER (2 Zeilen x:..., y:...) ---
    let Some(data) = transform.screen_to_data(cursor) else {
        return;
    };
    let x_text = plot.x_axis().format_value(data.x);
    let y_text = plot.y_axis().format_value(data.y);
    let label = format!("x: {x_text}\ny: {y_text}");

    // --- GPUI-PORT FIX 2: FREIER RAUM HOVER (2 Zeilen) ---
    // Eine schmälere und flachere Box für reines Koordinaten-Hovern
    let size = (90.0, 34.0);
    
    let mut origin = ScreenPoint::new(cursor.x + 12.0, cursor.y + 12.0);
    if origin.x + size.0 > plot_rect.max.x {
        origin.x = cursor.x - size.0 - 12.0;
    }
    if origin.y + size.1 > plot_rect.max.y {
        origin.y = cursor.y - size.1 - 12.0;
    }
    origin = clamp_point(origin, plot_rect, size);

    render.push(RenderCommand::Rect {
        rect: ScreenRect::new(
            origin,
            ScreenPoint::new(origin.x + size.0, origin.y + size.1),
        ),
        style: RectStyle {
            fill: theme.hover_bg,
            stroke: theme.hover_border,
            stroke_width: 1.0,
            dashed: false,
        },
    });

    for (index, line) in label.lines().enumerate() {
        let line_y = origin.y + index as f32 * 14.0 + 4.0;
        render.push(RenderCommand::Text {
            position: ScreenPoint::new(origin.x + 6.0, line_y),
            text: line.to_string(),
            style: TextStyle {
                color: theme.axis,
                size: 11.0,
            },
        });
    }
}

fn build_linked_cursor(
    render: &mut RenderList,
    plot: &Plot,
    state: &PlotUiState,
    transform: &Transform,
    plot_rect: ScreenRect,
    measurer: &EstimatedTextMeasurer,
) {
    // Wenn kein verlinktes X vorliegt, brechen wir ab
    let Some(x) = state.linked_cursor_x else {
        return;
    };
 // --- COUPLING-FIX ÜBER EINDEUTIGE PLOT-ID ---
             // --- NEUER FIX ÜBER LOKALES HOVER-TARGET ---
             /*
 // --- NEUER ANTEIL: ACHSEN-BEREICH DEBUG ---
    println!(
        "[DEBUG: {}] Empfangenes verlinktes X: {:.2} | Lokaler X-Viewport des Plots: {:.2} bis {:.2}",
        plot.id, 
        x, 
        transform.viewport().x.min, 
        transform.viewport().x.max
    );             
  */  
                // --- DER ENTKOPPELTE LOCAL-PLOT FIX ---
    if plot.is_mouse_actively_inside {
        return;
    }

                        // --- AUSFALLSICHERER LOCAL-POINTER FIX ---
  //  if state.is_mouse_inside {
        // Die Maus ist physisch in diesem Plot aktiv.
        // Das lokale Standard-Hover zieht, verlinkter Cursor bricht ab.
       // return;
  //  }
/*        
    // --- FINALER ID-FIX: PRÜFEN OB DIESER PLOT DIE MAUS HÄLT ---
    if let Some(ref hovered_id) = state.hovered_plot_id {
        if hovered_id == &plot.id {
            // Die Maus ist physisch in diesem Plot. 
            // Das lokale Hover-Popup (falls aktiv) zieht, wir zeichnen hier keinen verlinkten Cursor.
            return;
        }
    }
  */           
   /*            
    if state.hover_target.is_some() {
        println!(
            "[DEBUG] ABGEBROCHEN: Dieser Plot ({}) hat ein echtes lokales Hover-Target unter der Maus! plot_rect: {:?}", 
            plot.id, plot_rect
        );
        return;
    } else {
        println!(
            "[DEBUG] ZEICHNE INFOBOX: Dieser Plot ({}) hat KEIN lokales Target -> Zeichne verlinkte Infobox! plot_rect: {:?}", 
            plot.id, plot_rect
        );
    }
*/
        
    // --- DEBUG 1: PRÜFEN OB DER FRÜHE ABBRUCH TRAGIERT ---
  /*  if state.hover.is_some() {
        println!("[DEBUG] linked_cursor abgebrochen, weil state.hover aktiv ist! plot_rect: {:?}", plot_rect);
        return;
    }
*/
    // Numerisch stabile Transformation über die Mitte der Y-Achse
    let y_center = (transform.viewport().y.min + transform.viewport().y.max) * 0.5;
    let Some(screen_x) = transform
        .data_to_screen(crate::geom::Point::new(x, y_center))
        .map(|point| point.x)
    else {
        println!("[DEBUG] Transformation fehlgeschlagen für x={} bei rect: {:?}", x, plot_rect);
        return;
    };

    // Liegt das X außerhalb des sichtbaren Plot-Bereichs, brechen wir ab
    if screen_x < plot_rect.min.x || screen_x > plot_rect.max.x {
        println!(
            "[DEBUG] screen_x ({:.2}) außerhalb plot_rect ({:.2} - {:.2})", 
            screen_x, plot_rect.min.x, plot_rect.max.x
        );
        return;
    }

    let theme = plot.theme();

    // --- GPUI-BUGFIX 2: VERTIKALE FADENKREUZ-LINIE WIEDER EINFÜGEN ---
    render.push(RenderCommand::ClipRect(plot_rect));
    render.push(RenderCommand::LineSegments {
        segments: vec![LineSegment::new(
            ScreenPoint::new(screen_x, plot_rect.min.y),
            ScreenPoint::new(screen_x, plot_rect.max.y),
        )],
        style: LineStyle {
            color: with_alpha(theme.axis, super::constants::LINK_CURSOR_ALPHA),
            width: super::constants::LINK_CURSOR_WIDTH,
        },
    });
    render.push(RenderCommand::ClipEnd);

    // 1. Sammle die Textzeilen für alle AKTIVEN (sichtbaren) Serien dieses Plots
    let mut lines = Vec::new();
    lines.push(format!("x:{}", plot.x_axis().format_value(x)));

    let mut hidden = 0usize;
    for series in plot.series() {
        if !series.is_visible() {
            continue;
        }
        let point = series.with_store(|store| {
            let data = store.data();
            data.nearest_index_by_x(x)
                .and_then(|index| data.point(index))
        });
        if let Some(point) = point {
            if lines.len() <= 6 {
                lines.push(format!(
                    "{}:{}",
                    series.name(),
                    plot.y_axis().format_value(point.y)
                ));
            } else {
                hidden += 1;
            }
        }
    }
    if hidden > 0 {
        lines.push(format!("+{hidden} more"));
    }
    
    if lines.is_empty() {
        println!("[DEBUG] Keine Linien/Serien-Daten zum Anzeigen gefunden für rect: {:?}", plot_rect);
        return;
    }

    let label = lines.join("\n");
    
    // 2. EXAKTE GEOMETRIE-WEITUNG
    let font_size = 11.0;    
    let line_height = 14.0;
    
    let box_height = (lines.len() as f32 * line_height) + 8.0; 
    let box_width = 165.0; 
    let size = (box_width, box_height);

    // Platzierung der Info-Box (10px Offset von der Fadenkreuz-Linie)
    let mut origin = ScreenPoint::new(screen_x + 10.0, plot_rect.min.y + 10.0);
    if origin.x + size.0 > plot_rect.max.x {
        origin.x = screen_x - size.0 - 10.0;
    }
    
    let clamped_origin = clamp_point(origin, plot_rect, size);
/*
    // --- DEBUG 2: EXAKTE KOORDINATEN DER INFOBOX IM PLOT ---
    println!(
        "[DEBUG] INFOBOX BERECHNET -> screen_x: {:.1}, plot_rect: (min_y: {:.1}, max_y: {:.1}), Ursprung vor Clamp: ({:.1}, {:.1}), nach Clamp: ({:.1}, {:.1})",
        screen_x, plot_rect.min.y, plot_rect.max.y, origin.x, origin.y, clamped_origin.x, clamped_origin.y
    );
*/
    origin = clamped_origin;

    // 3. Zeichne das rechteckige Info-Popup
    render.push(RenderCommand::Rect {
        rect: ScreenRect::new(
            origin,
            ScreenPoint::new(origin.x + size.0, origin.y + size.1),
        ),
        style: RectStyle {
            fill: with_alpha(theme.hover_bg, 0.9),
            stroke: with_alpha(theme.hover_border, 0.9),
            stroke_width: 1.0,
            dashed: false,
        },
    });

    // 4. Render den Text zeilenweise
    for (index, line) in label.lines().enumerate() {
        let line_y = origin.y + (index as f32 * line_height) + 4.0;
        render.push(RenderCommand::Text {
            position: ScreenPoint::new(origin.x + 6.0, line_y),
            text: line.to_string(),
            style: TextStyle {
                color: theme.axis,
                size: font_size,
            },
        });
    }
}

fn build_linked_brush(
    render: &mut RenderList,
    plot: &Plot,
    state: &PlotUiState,
    transform: &Transform,
    plot_rect: ScreenRect,
) {
    let Some(range) = state.linked_brush_x else {
        return;
    };
    if state.selection_rect.is_some() {
        return;
    }

    // --- GPUI-BUGFIX 3: NUMERISCH STABILES BRUSH-TRANSFORM ---
    // Genau wie beim Cursor nutzen wir hier die Mitte des Y-Viewports, damit die X-Pixel-Berechnung
    // des Pinsels beim Skalieren der Y-Achse nicht in ungültige Grenzbereiche gerät.
    let y_center = (transform.viewport().y.min + transform.viewport().y.max) * 0.5;
    let Some(start_x) = transform
        .data_to_screen(crate::geom::Point::new(range.min, y_center))
        .map(|point| point.x)
    else {
        return;
    };
    let Some(end_x) = transform
        .data_to_screen(crate::geom::Point::new(range.max, y_center))
        .map(|point| point.x)
    else {
        return;
    };

    let min_x = start_x.min(end_x).clamp(plot_rect.min.x, plot_rect.max.x);
    let max_x = start_x.max(end_x).clamp(plot_rect.min.x, plot_rect.max.x);
    if (max_x - min_x).abs() < 1.0 {
        return;
    }

    let theme = plot.theme();
    render.push(RenderCommand::ClipRect(plot_rect));
    render.push(RenderCommand::Rect {
        rect: ScreenRect::new(
            ScreenPoint::new(min_x, plot_rect.min.y),
            ScreenPoint::new(max_x, plot_rect.max.y),
        ),
        style: RectStyle {
            fill: with_alpha(theme.selection_fill, super::constants::LINK_BRUSH_FILL_ALPHA),
            stroke: with_alpha(theme.selection_border, super::constants::LINK_BRUSH_BORDER_ALPHA),
            stroke_width: 1.0,
            dashed: false,
        },
    });
    render.push(RenderCommand::ClipEnd);
}

fn build_legend(
    render: &mut RenderList,
    plot: &Plot,
    state: &mut PlotUiState,
    plot_rect: ScreenRect,
    measurer: &EstimatedTextMeasurer,
) {
    let theme = plot.theme();
    let series_list = plot.series();
    if series_list.is_empty() {
        state.legend_layout = None;
        return;
    }

    let font_size = LEGEND_FONT_SIZE;
    let line_height = LEGEND_LINE_HEIGHT;
    let padding = LEGEND_PADDING;
    let text_start_x = padding
        + LEGEND_TOGGLE_DIAMETER
        + LEGEND_TOGGLE_GAP
        + LEGEND_SWATCH_WIDTH
        + LEGEND_SWATCH_GAP;
    let mut max_width: f32 = 0.0;
    for series in series_list {
        let size = measurer.measure(series.name(), font_size);
        max_width = max_width.max(size.0);
    }
    let legend_width = text_start_x + max_width + padding;
    let legend_height = series_list.len() as f32 * line_height + padding * 2.0;

    let mut origin = ScreenPoint::new(
        plot_rect.max.x - legend_width - padding,
        plot_rect.min.y + padding,
    );
    origin = clamp_point(origin, plot_rect, (legend_width, legend_height));
    let legend_rect = ScreenRect::new(
        origin,
        ScreenPoint::new(origin.x + legend_width, origin.y + legend_height),
    );

    render.push(RenderCommand::Rect {
        rect: legend_rect,
        style: RectStyle {
            fill: theme.legend_bg,
            stroke: theme.legend_border,
            stroke_width: 1.0,
            dashed: false,
        },
    });

    let mut entries = Vec::with_capacity(series_list.len());
    for (idx, series) in series_list.iter().enumerate() {
        let row_y = origin.y + padding + idx as f32 * line_height;
        let row_rect = ScreenRect::new(
            ScreenPoint::new(origin.x, row_y),
            ScreenPoint::new(origin.x + legend_width, row_y + line_height),
        );
        let row_center_y = row_y + line_height * 0.5;
        let toggle_origin = ScreenPoint::new(
            origin.x + padding,
            row_center_y - LEGEND_TOGGLE_DIAMETER * 0.5,
        );
        let toggle_rect = ScreenRect::new(
            toggle_origin,
            ScreenPoint::new(
                toggle_origin.x + LEGEND_TOGGLE_DIAMETER,
                toggle_origin.y + LEGEND_TOGGLE_DIAMETER,
            ),
        );
        entries.push(LegendEntry {
            series_id: series.id(),
            row_rect,
        });

        let visible = series.is_visible();
        let series_color = series_color(series);
        let swatch_color = if visible {
            series_color
        } else {
            with_alpha(series_color, LEGEND_HIDDEN_ALPHA)
        };
        let text_color = if visible {
            theme.axis
        } else {
            with_alpha(theme.axis, LEGEND_TEXT_HIDDEN_ALPHA)
        };
        let ring_color = if visible {
            with_alpha(theme.axis, 0.7)
        } else {
            with_alpha(theme.axis, 0.45)
        };
        let fill_color = if visible {
            series_color
        } else {
            theme.legend_bg
        };
        let toggle_center = ScreenPoint::new(
            toggle_rect.min.x + LEGEND_TOGGLE_DIAMETER * 0.5,
            toggle_rect.min.y + LEGEND_TOGGLE_DIAMETER * 0.5,
        );

        render.push(RenderCommand::Points {
            points: vec![toggle_center],
            style: MarkerStyle {
                color: ring_color,
                size: LEGEND_TOGGLE_DIAMETER,
                shape: MarkerShape::Circle,
            },
        });
        render.push(RenderCommand::Points {
            points: vec![toggle_center],
            style: MarkerStyle {
                color: fill_color,
                size: LEGEND_TOGGLE_INNER_DIAMETER,
                shape: MarkerShape::Circle,
            },
        });

        let swatch_start = ScreenPoint::new(toggle_rect.max.x + LEGEND_TOGGLE_GAP, row_center_y);
        let swatch_end = ScreenPoint::new(swatch_start.x + LEGEND_SWATCH_WIDTH, row_center_y);
        render.push(RenderCommand::LineSegments {
            segments: vec![LineSegment::new(swatch_start, swatch_end)],
            style: LineStyle {
                color: swatch_color,
                width: 2.0,
            },
        });
        let text_y = row_y + (line_height - font_size) * 0.5;
        render.push(RenderCommand::Text {
            position: ScreenPoint::new(swatch_end.x + LEGEND_SWATCH_GAP, text_y),
            text: series.name().to_string(),
            style: TextStyle {
                color: text_color,
                size: font_size,
            },
        });
    }

    state.legend_layout = Some(LegendLayout {
        rect: legend_rect,
        entries,
    });
}

#[derive(Debug, Clone)]
struct PinLabel {
    screen: ScreenPoint,
    label: String,
    size: (f32, f32),
}

fn marker_style_and_size(series: &Series) -> (MarkerStyle, f32) {
    match series.kind() {
        SeriesKind::Line(line) => (
            MarkerStyle {
                color: line.color,
                size: 6.0,
                shape: MarkerShape::Circle,
            },
            6.0,
        ),
        SeriesKind::Scatter(marker) => (
            MarkerStyle {
                color: marker.color,
                size: marker.size.max(6.0),
                shape: marker.shape,
            },
            marker.size.max(6.0),
        ),
    }
}

fn cluster_pin_labels(labels: &[PinLabel], radius: f32) -> Vec<Vec<usize>> {
    let radius_sq = radius * radius;
    let mut visited = vec![false; labels.len()];
    let mut clusters: Vec<Vec<usize>> = Vec::new();

    for start in 0..labels.len() {
        if visited[start] {
            continue;
        }
        visited[start] = true;
        let mut cluster = Vec::new();
        let mut stack = vec![start];
        while let Some(index) = stack.pop() {
            cluster.push(index);
            for next in 0..labels.len() {
                if visited[next] {
                    continue;
                }
                if distance_sq(labels[index].screen, labels[next].screen) <= radius_sq {
                    visited[next] = true;
                    stack.push(next);
                }
            }
        }
        clusters.push(cluster);
    }

    clusters
}

fn cluster_center(labels: &[PinLabel], cluster: &[usize]) -> ScreenPoint {
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    for index in cluster {
        let screen = labels[*index].screen;
        sum_x += screen.x;
        sum_y += screen.y;
    }
    let count = cluster.len().max(1) as f32;
    ScreenPoint::new(sum_x / count, sum_y / count)
}

fn pin_label_candidates(screen: ScreenPoint, size: (f32, f32), offset: f32) -> [ScreenPoint; 6] {
    [
        ScreenPoint::new(screen.x + offset, screen.y + offset),
        ScreenPoint::new(screen.x + offset, screen.y - size.1 - offset),
        ScreenPoint::new(screen.x - size.0 - offset, screen.y + offset),
        ScreenPoint::new(screen.x - size.0 - offset, screen.y - size.1 - offset),
        ScreenPoint::new(screen.x - size.0 * 0.5, screen.y - size.1 - offset),
        ScreenPoint::new(screen.x - size.0 * 0.5, screen.y + offset),
    ]
}

fn place_label(
    screen: ScreenPoint,
    size: (f32, f32),
    plot_rect: ScreenRect,
    offset: f32,
    placed: &[ScreenRect],
) -> Option<(ScreenPoint, ScreenRect)> {
    for origin in pin_label_candidates(screen, size, offset) {
        let origin = clamp_point(origin, plot_rect, size);
        let rect = ScreenRect::new(
            origin,
            ScreenPoint::new(origin.x + size.0, origin.y + size.1),
        );
        if !rect_intersects_any(rect, placed) {
            return Some((origin, rect));
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn push_label_with_leader(
    render: &mut RenderList,
    rect: ScreenRect,
    origin: ScreenPoint,
    screen: ScreenPoint,
    label: &str,
    font_size: f32,
    line_height: f32,
    theme: &Theme,
) {
    let anchor = ScreenPoint::new(
        screen.x.clamp(rect.min.x, rect.max.x),
        screen.y.clamp(rect.min.y, rect.max.y),
    );
    render.push(RenderCommand::LineSegments {
        segments: vec![LineSegment::new(screen, anchor)],
        style: LineStyle {
            color: theme.pin_border,
            width: 1.0,
        },
    });
    render.push(RenderCommand::Rect {
        rect,
        style: RectStyle {
            fill: theme.pin_bg,
            stroke: theme.pin_border,
            stroke_width: 1.0,
            dashed: false,
        },
    });
    for (index, line) in label.lines().enumerate() {
        let line_y = origin.y + index as f32 * line_height + 2.0;
        render.push(RenderCommand::Text {
            position: ScreenPoint::new(origin.x + 4.0, line_y),
            text: line.to_string(),
            style: TextStyle {
                color: theme.axis,
                size: font_size,
            },
        });
    }
}

fn axis_title_text(axis: &AxisConfig) -> Option<String> {
    match (axis.title(), axis.units()) {
        (Some(title), Some(units)) => Some(format!("{title} ({units})")),
        (Some(title), None) => Some(title.to_string()),
        (None, Some(units)) => Some(units.to_string()),
        (None, None) => None,
    }
}

fn series_color(series: &Series) -> Color {
    match series.kind() {
        SeriesKind::Line(style) => style.color,
        SeriesKind::Scatter(style) => style.color,
    }
}

fn with_alpha(color: Color, alpha: f32) -> Color {
    Color {
        a: (color.a * alpha).clamp(0.0, 1.0),
        ..color
    }
}
