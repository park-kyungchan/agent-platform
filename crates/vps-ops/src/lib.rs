pub mod adapters;
pub mod cli;
pub mod domain;
pub mod host;
pub mod runner;
pub mod update;
pub mod util;
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
