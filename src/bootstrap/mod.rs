//! Application bootstrap: database, dynamic config, services, and HTTP stack.

pub mod app;
pub mod db;
pub mod logging;

pub use app::{AppBuildResult, ShutdownHandle};
pub use db::connect_pool;
