pub type Res<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
type Style = (vt100::Color, vt100::Color, bool, bool, bool);

mod app;
mod config;
mod term;
mod ui;

pub use app::{App, TermGuard};
pub use config::{Action, Config, key_to_bytes};
pub use term::Pane;
pub use ui::{Rect, blit, draw_box, fill, popup_rect, put_text, split_rects};
