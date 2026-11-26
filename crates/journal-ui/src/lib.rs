slint::include_modules!();

pub mod charts;
pub mod controller;
pub mod screenshots;
pub mod theme;

pub use controller::{UiController, run_desktop_app};
pub use screenshots::render_screenshots_headless;
