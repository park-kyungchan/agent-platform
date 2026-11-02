//! S03 Linux public-instruction selectors. No vendor settings or runtime execution.
#[doc(hidden)]
pub mod cli;
mod doctor;
mod filesystem;
mod install;
mod recovery;
mod types;

pub use doctor::doctor;
pub use install::{apply, plan};
pub use recovery::recover;
pub use types::*;
