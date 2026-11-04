//! Advanced live plot: two stacked, x-linked plots with live streaming data,
//! scatter events, callback baseline, legend, hover and pins.
//!
//! Port of `gpui-liveplot/examples/advanced.rs` to iced idioms: two canvas
//! programs in a column, a `time::every` subscription pushing new samples
//! into both streams.

//use iced::widget::text;
use iced::{Subscription, Task, Widget};
use iced::Renderer;
 // use iced::widget::container;
  use iced::widget::{container, column};
//use iced::Color;
use iced::widget::mouse_area; // mouse_area importieren
use iced_liveplot::axis::AxisConfig;
use iced_liveplot::render::{LineStyle, MarkerShape, MarkerStyle};
use iced_liveplot::series::{Series, SeriesKind};
use iced_liveplot::style::Theme;
use iced_liveplot::view::View;
use iced_liveplot::widget::{self, Program};
use iced_liveplot::{Plot, Range};

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .title("advanced — iced-liveplot")
        .subscription(App::subscription)
        .window_size(iced::Size {
            width: 1_000.0,
            height: 740.0,
        })
        .run()
}

#[derive(Debug, Clone)]
pub enum Message {
    Top(widget::Message),
    Bottom(widget::Message),
    Tick,
}

struct App {
    top: Program,
    bottom: Program,
    stream_a: Series,
    stream_b: Series,
    phase: f64,
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let mut stream_a = Series::line("stream-A");
        let mut stream_b = Series::line("stream-B");

        for i in 0..1_000 {
            let phase = i as f64 * 0.02;
            let _ = stream_a.push_y((phase * 0.9).sin() + 0.2 * (phase * 0.13).cos());
            let _ = stream_b.push_y((phase * 0.45).cos() * 1.15 + 0.15 * (phase * 0.09).sin());
        }

        let events = Series::from_iter_points(
            "events(scatter)",
            (0..200).map(|i| {
                let x = i as f64 * 80.0 + 40.0;
                let y = (x * 0.02).sin() * 0.9;
                iced_liveplot::Point::new(x, y)
            }),
            SeriesKind::Scatter(MarkerStyle {
                color: iced::Color {
                    r: 0.95,
                    g: 0.25,
                    b: 0.55,
                    a: 1.0,
                },
                size: 5.0,
                shape: MarkerShape::Circle,
            }),
        );

        let baseline = Series::from_explicit_callback(
            "baseline(callback)",
            |x| (x * 0.015).sin() * 0.4,
            Range::new(0.0, 25_000.0),
            5_000,
            SeriesKind::Line(LineStyle {
                color: iced::Color {
                    r: 0.45,
                    g: 0.45,
                    b: 0.5,
                    a: 0.8,
                },
                width: 1.0,
            }),
        );

        let mut top_plot = Plot::builder()
            .theme(Theme::dark())
            .x_axis(AxisConfig::builder().title("Sample").build())
            .y_axis(AxisConfig::builder().title("Top: stream + events").build())
            .view(View::FollowLastN { points: 2_000 })
            .build();
        top_plot.add_series(&stream_a);
        top_plot.add_series(&events);

        let mut bottom_plot = Plot::builder()
            .theme(Theme::dark())
            .x_axis(AxisConfig::builder().title("Sample").build())
            .y_axis(
                AxisConfig::builder()
                    .title("Bottom: stream + baseline")
                    .build(),
            )
            .view(View::FollowLastNXY { points: 2_000 })
            .build();
        bottom_plot.add_series(&stream_b);
        bottom_plot.add_series(&baseline);

        (
            Self {
                top: Program::new(top_plot),
                bottom: Program::new(bottom_plot),
                stream_a,
                stream_b,
                phase: 0.0,
            },
            Task::none(),
        )
    }

fn update(&mut self, message: Message) -> Task<Message> {
    match message {
        Message::Tick => {
            // 1. Daten in den lokalen Puffern der App erweitern
            let new_a: Vec<f64> = (0..120).map(|_| {
                let y = (self.phase * 0.9).sin() + 0.2 * (self.phase * 0.13).cos();
                self.phase += 0.02;
                y
            }).collect();

            let new_b: Vec<f64> = (0..120).map(|_| {
                let y = (self.phase * 0.45).cos() * 1.15 + 0.15 * (self.phase * 0.09).sin();
                self.phase += 0.02;
                y
            }).collect();

            let _ = self.stream_a.extend_y(new_a.clone());
            let _ = self.stream_b.extend_y(new_b.clone());

            // 2. Daten synchron in die aktiven Plot-Modelle hineinreichen
            self.top.with_plot_mut(|plot| {
                if let Some(series) = plot.series_mut().iter_mut().find(|s| s.name() == "stream-A") {
                    let _ = series.extend_y(new_a);
                }
            });

            self.bottom.with_plot_mut(|plot| {
                if let Some(series) = plot.series_mut().iter_mut().find(|s| s.name() == "stream-B") {
                    let _ = series.extend_y(new_b);
                }
            });

            self.top.request_redraw();
            self.bottom.request_redraw();
            Task::none()
        }

        // Nachrichten des OBEREN Plots verarbeiten
        Message::Top(top_msg) => {
            match top_msg {
                // HIER: GPUI-Doppelklick-Reset spiegeln (link_reset: true)
                widget::Message::ResetView => {
                    self.top.with_plot_mut(|plot| { plot.reset_view(); });
                    self.bottom.with_plot_mut(|plot| { plot.reset_view(); });
                }
                // Alle anderen Nachrichten (ToggleSeries, Clicks, Drags etc.)
                _ => {}
            }
            self.top.request_redraw();
            self.bottom.request_redraw();
            Task::none()
        }

        // Nachrichten des UNTEREN Plots verarbeiten
        Message::Bottom(bottom_msg) => {
            match bottom_msg {
                // HIER: GPUI-Doppelklick-Reset spiegeln (link_reset: true)
                widget::Message::ResetView => {
                    self.top.with_plot_mut(|plot| { plot.reset_view(); });
                    self.bottom.with_plot_mut(|plot| { plot.reset_view(); });
                }
                // Alle anderen Nachrichten
                _ => {}
            }
            self.top.request_redraw();
            self.bottom.request_redraw();
            Task::none()
        }
    }
}







// In deiner App (kann jetzt wieder so clean aussehen!):


fn view(&self) -> impl Widget<Message> + use<'_> {
    column![
        // Oberer Plot in eine MouseArea packen
        mouse_area(
            iced_liveplot::widget::plot(&self.top)
                .height(iced::Length::FillPortion(1))
                .map(Message::Top)
        )
        // Sendet bei einem echten Doppelklick direkt das Signal an die App
        .on_double_click(Message::Top(widget::Message::ResetView)), 

        // Unterer Plot in eine MouseArea packen
        mouse_area(
            iced_liveplot::widget::plot(&self.bottom)
                .height(iced::Length::FillPortion(1))
                .map(Message::Bottom)
        )
        // Sendet bei einem echten Doppelklick direkt das Signal an die App
        .on_double_click(Message::Bottom(widget::Message::ResetView)),
    ]
    .spacing(0)
    .width(iced::Length::Fill)
    .height(iced::Length::Fill)
}





    fn subscription(&self) -> Subscription<Message> {
        iced::time::every(iced::time::milliseconds(16)).map(|_| Message::Tick)
    }
}
