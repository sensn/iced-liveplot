//! Advanced live plot: two stacked, x-linked plots with live streaming data,
//! scatter events, callback baseline, legend, hover and pins.
//!
//! Port of `gpui-liveplot/examples/advanced.rs` to iced idioms: two canvas
//! programs in a column, a `time::every` subscription pushing new samples
//! into both streams.

//use iced::widget::text;
use iced::{Subscription, Task, Widget};
//use iced::Renderer;
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

// Linking plots
use iced_liveplot::widget::PlotViewConfig; 
use iced_liveplot::widget::link::{PlotLinkGroup, PlotLinkOptions}; // KEIN LinkBinding hier!


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
        //let mut stream_a = Series::line("stream-A");
        //let mut stream_b = Series::line("stream-B");
        
         // 1. Initialisiere Stream A mit dem originalen Teal-Farbton
    let mut stream_a = Series::line("stream-A").with_kind(SeriesKind::Line(LineStyle {
        color: iced::Color {
            r: 0.2,
            g: 0.82,
            b: 0.95,
            a: 1.0,
        },
        width: 2.0,
    }));

    // 2. Initialisiere Stream B mit dem originalen Orange-Farbton
    let mut stream_b = Series::line("stream-B").with_kind(SeriesKind::Line(LineStyle {
        color: iced::Color {
            r: 0.95,
            g: 0.64,
            b: 0.28,
            a: 1.0,
        },
        width: 2.0,
    }));



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
        
         let config = PlotViewConfig {
        show_legend: true,
        show_hover: true,
        ..Default::default()
    };

    let link_group = PlotLinkGroup::new();
    let options = PlotLinkOptions {
        link_x: true,
        link_y: false,
        link_cursor: true,
        link_brush: true,
        link_reset: true,
    };

      // 2. Kette die Builder-Methoden an die Widgets
        let top_program = Program::new(top_plot)
            .with_config(config.clone())
            .with_link_group(link_group.clone(), options);

        let bottom_program = Program::new(bottom_plot)
            .with_config(config)
            .with_link_group(link_group, options);

        // 3. Setze die Variablen direkt in den Self-Block ein
        (
            Self {
                top: top_program,       // Nutzt das fertig verlinkte Top-Widget
                bottom: bottom_program, // Nutzt das fertig verlinkte Bottom-Widget
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
            // 1. Daten NUR in den lokalen Puffern der App erweitern
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

            let _ = self.stream_a.extend_y(new_a);
            let _ = self.stream_b.extend_y(new_b);

            // KEIN "2. Daten synchron in die aktiven Plot-Modelle hineinreichen"!
            
            self.top.request_redraw();
            self.bottom.request_redraw();
            Task::none()
        }

        // Nachrichten des OBEREN Plots verarbeiten
        Message::Top(top_msg) => {
            match top_msg {
                widget::Message::ResetView => {
                    self.top.with_plot_mut(|plot| { plot.reset_view(); });
                    self.bottom.with_plot_mut(|plot| { plot.reset_view(); });
                }
                _ => {}
            }
            self.top.request_redraw();
            self.bottom.request_redraw();
            Task::none()
        }

        // Nachrichten des UNTEREN Plots verarbeiten
        Message::Bottom(bottom_msg) => {
            match bottom_msg {
                widget::Message::ResetView => {
                    self.top.with_plot_mut(|plot| { plot.reset_view(); });
                    self.bottom.with_plot_mut(|plot| { plot.reset_view(); });
                }
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
    let dark_background = iced::Background::Color(iced::Color::from_rgb(0.1, 0.1, 0.12));
    
    container(
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
    ) // <-- HIER war die fehlende Klammer, die den container() Aufruf schließt!
    .width(iced::Length::Fill)
    .height(iced::Length::Fill)
    .style(move |_theme| container::Style {
        background: Some(dark_background),
        ..Default::default()
    })
}






    fn subscription(&self) -> Subscription<Message> {
        iced::time::every(iced::time::milliseconds(16)).map(|_| Message::Tick)
    }
}
