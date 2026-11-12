//! Minimal static plot: a single sine series, dark theme, legend and hover.
//!
//! Port of `gpui-liveplot/examples/basic.rs` to iced idioms:
//! purely event-driven redraws on user interaction (zero idle CPU/GPU usage).

use iced::{Task, Widget};

use iced_liveplot::axis::AxisConfig;
use iced_liveplot::render::LineStyle;
use iced_liveplot::series::{Series, SeriesKind};
use iced_liveplot::style::Theme;
use iced_liveplot::widget::{self, Message as PlotMessage, Program};
use iced_liveplot::Plot;

fn main() -> iced::Result {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

    iced::application(App::new, App::update, App::view)
        .title("basic — iced-liveplot")
        .window_size(iced::Size {
            width: 720.0,
            height: 480.0,
        })
        .run()
}

#[derive(Debug, Clone)]
pub enum Message {
    /// A message from the plot widget (interaction or toggle).
    Plot(PlotMessage),
}

struct App {
    program: Program,
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let series = Series::from_iter_y(
            "signal",
            (0..400).map(|i| {
                let x = i as f64 * 0.03;
                x.sin()
            }),
            SeriesKind::Line(LineStyle {
                color: iced::Color {
                    r: 0.2,
                    g: 0.75,
                    b: 0.95,
                    a: 1.0,
                },
                width: 2.0,
            }),
        );

        let mut plot = Plot::builder()
            .id("top_plot")
            .theme(Theme::dark())
            .x_axis(AxisConfig::builder().title("Sample").build())
            .y_axis(AxisConfig::builder().title("Amplitude").build())
            .build();
        plot.add_series(&series);

        (
            Self {
                program: Program::new(plot),
            },
            Task::none(),
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Plot(widget::Message::ResetView) => {
                self.program.with_plot_mut(|plot| {
                    plot.reset_view();
                });
                self.program.request_redraw();
                Task::none()
            }
            // The widget already requested a repaint for hover, pan, zoom, and pins.
            Message::Plot(_) => Task::none(),
        }
    }

    fn view(&self) -> impl Widget<Message> + use<'_> {
        widget::plot(&self.program)
            .width(iced::Length::Fill)
            .height(iced::Length::Fill)
            .map(Message::Plot)
    }
}
