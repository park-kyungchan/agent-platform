//! Pure rendering plus explicitly scoped Linux public-instruction installation.
//! Native installation is an explicit separate Linux API; render remains pure.

#[cfg(target_os = "linux")]
pub mod native;

mod renderer;
mod types;

pub use renderer::render;
pub use types::*;

#[cfg(target_os = "linux")]
pub mod pane;

pub mod work;

pub mod review;

pub mod run;
