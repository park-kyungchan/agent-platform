//! Pure proposed native instruction artifacts. No environment or filesystem discovery.
//! The S02 foundation deliberately returns RendererUnimplemented until the leaf is supplied.

mod renderer;
mod types;

pub use renderer::render;
pub use types::*;
