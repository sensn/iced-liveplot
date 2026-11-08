//! Live plotting for iced, ported from `gpui-liveplot`.

pub mod automation;
pub mod axis;
pub mod audio;
pub mod datasource;
pub mod geom;
pub mod interaction;
pub mod midi;
pub mod plot;
pub mod region;
pub mod render;
pub mod selection;
pub mod series;
pub mod snap;
pub mod style;
pub mod transform;
pub mod view;
pub mod widget;

pub use automation::{AutomationLane, AutomationPoint, AutomationStyle, AutomationTrack, parameters};
pub use audio::{AudioRegion, AudioTrack, WaveformStyle, render_waveform};
pub use geom::Point;
pub use midi::{MidiNote, MidiSequence, MidiTrack, PianoRollStyle, is_black_key, midi_note_name, render_piano_roll};
pub use plot::{Plot, PlotBuilder};
pub use region::{DataRegion, RegionId, RegionKind, RegionLayer, RegionStyle, Range as RegionRange};
pub use selection::Selection;
pub use series::{Series, SeriesId, SeriesKind};
pub use snap::{GridSpec, SnapConfig, SnapEngine, SnapResult, SnapType, TimeBase};
pub use style::Theme;
pub use transform::Transform;
pub use view::{Range, View, Viewport};
pub use widget::{KeyBindings, PlotAction, StandardAction, Tool, ToolRegistry, ToolState};