//! Live plotting for iced, ported from `gpui-liveplot`.

pub mod axis;
pub mod datasource;
pub mod geom;
pub mod interaction;
pub mod plot;
pub mod render;
pub mod widget;
pub mod series;
pub mod style;
pub mod transform;
pub mod view;

pub use geom::Point;
pub use plot::{Plot, PlotBuilder};
pub use series::Series;
pub use style::Theme;
pub use view::{Range, View, Viewport};
