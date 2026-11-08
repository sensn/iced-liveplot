//! Execution of backend-agnostic [`render::RenderCommand`]s on an iced
//! [`canvas::Frame`].
//!
//! This replaces the GPUI `paint` module: instead of issuing immediate paint
//! calls, each command is translated into canvas [`Path`]s and text runs that
//! the iced renderer records into cached geometry.

use iced::widget::canvas::{self, Path, Stroke, Text as CanvasText};
use iced::{Point, Rectangle, Size};

use crate::geom::{ScreenPoint, ScreenRect};
use crate::render::{
    LineSegment, LineStyle, MarkerShape, MarkerStyle, RectStyle,
    RenderList, RenderCommand, TextStyle,
};

/// Paint a full render list into `frame`.
pub(crate) fn paint(frame: &mut canvas::Frame, list: &RenderList) {
    paint_commands(frame, list.commands());
}

fn paint_commands(frame: &mut canvas::Frame, commands: &[RenderCommand]) {
    let mut index = 0;
    while index < commands.len() {
        match &commands[index] {
            RenderCommand::ClipRect(rect) => {
                // Find the matching `ClipEnd`, accounting for nested clip
                // regions so the inner commands stay within their own region.
                let mut depth = 1;
                let mut end = index + 1;
                while end < commands.len() {
                    match commands[end] {
                        RenderCommand::ClipRect(_) => depth += 1,
                        RenderCommand::ClipEnd => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    end += 1;
                }

                let region = to_rectangle(*rect);
                frame.with_clip(region, |clipped| {
                    paint_commands(clipped, &commands[index + 1..end]);
                });
                index = end + 1;
            }
            RenderCommand::ClipEnd => {
                // Unbalanced end: ignore and continue.
                index += 1;
            }
            command => {
                paint_one(frame, command);
                index += 1;
            }
        }
    }
}

fn paint_one(frame: &mut canvas::Frame, command: &RenderCommand) {
    match command {
        RenderCommand::ClipRect(_) | RenderCommand::ClipEnd => unreachable!(
            "clip commands are handled by `paint_commands`"
        ),
        RenderCommand::LineSegments { segments, style } => {
            paint_line_segments(frame, segments, style)
        }
        RenderCommand::Points { points, style } => paint_points(frame, points, style),
        RenderCommand::Rect { rect, style } => paint_rect(frame, rect, style),
        RenderCommand::Text {
            position,
            text,
            style,
        } => paint_text(frame, position, text, style),
        RenderCommand::Path { path, fill, stroke, stroke_width } => {
            if let Some(fill) = fill {
                frame.fill(path, *fill);
            }
            if let Some(stroke) = stroke {
                frame.stroke(path, canvas::Stroke::default().with_color(*stroke).with_width(*stroke_width));
            }
        }
    }
}

fn paint_line_segments(frame: &mut canvas::Frame, segments: &[LineSegment], style: &LineStyle) {
    let Some(first) = segments.first() else {
        return;
    };

    let mut builder = canvas::path::Builder::new();
    builder.move_to(to_point(first.start));
    builder.line_to(to_point(first.end));
    for segment in &segments[1..] {
        builder.move_to(to_point(segment.start));
        builder.line_to(to_point(segment.end));
    }
    let path = builder.build();
    frame.stroke(&path, stroke_of(style.color, style.width));
}

fn paint_points(frame: &mut canvas::Frame, points: &[ScreenPoint], style: &MarkerStyle) {
    for point in points {
        let center = to_point(*point);
        let half = style.size / 2.0;

        let path = match style.shape {
            MarkerShape::Circle => Path::circle(center, half),
            MarkerShape::Square => Path::rectangle(
                Point::new(center.x - half, center.y - half),
                Size::new(style.size, style.size),
            ),
            MarkerShape::Cross => {
                let mut builder = canvas::path::Builder::new();
                // Top-left to bottom-right.
                builder.move_to(Point::new(center.x - half, center.y - half));
                builder.line_to(Point::new(center.x + half, center.y + half));
                // Bottom-left to top-right.
                builder.move_to(Point::new(center.x - half, center.y + half));
                builder.line_to(Point::new(center.x + half, center.y - half));
                builder.build()
            }
        };

        match style.shape {
            MarkerShape::Cross => frame.stroke(&path, stroke_of(style.color, 1.0)),
            MarkerShape::Circle | MarkerShape::Square => frame.fill(&path, style.color),
        }
    }
}

fn paint_rect(frame: &mut canvas::Frame, rect: &ScreenRect, style: &RectStyle) {
    let (position, size) = rect_to_parts(rect);
    let path = Path::rectangle(position, size);
    frame.fill(&path, style.fill);
    frame.stroke(&path, stroke_of(style.stroke, style.stroke_width));
}

fn paint_text(frame: &mut canvas::Frame, position: &ScreenPoint, text: &str, style: &TextStyle) {
    frame.fill_text(CanvasText {
        content: text.to_owned(),
        position: to_point(*position),
        color: style.color,
        size: iced::Pixels(style.size),
        // HIER ERGÄNZEN: Begrenzt die Box horizontal auf z.B. 90 oder 100 Pixel,
        // wodurch der Text sauber in ein kompaktes Rechteck umbricht.
        max_width: 100.0, 
        ..Default::default()
    });
}


fn to_point(p: ScreenPoint) -> Point {
    Point::new(p.x, p.y)
}

fn to_rectangle(rect: ScreenRect) -> Rectangle<f32> {
    let (position, size) = rect_to_parts(&rect);
    Rectangle::new(position, size)
}

fn rect_to_parts(rect: &ScreenRect) -> (Point, Size) {
    let x = rect.min.x.min(rect.max.x);
    let y = rect.min.y.min(rect.max.y);
    (
        Point::new(x, y),
        Size::new((rect.max.x - rect.min.x).abs(), (rect.max.y - rect.min.y).abs()),
    )
}

fn stroke_of(color: iced::Color, width: f32) -> Stroke<'static> {
    Stroke::default().with_color(color).with_width(width)
}
