//! Statistics: dashboard aggregates and flexible query builder.

mod builder;
mod cache;
mod dashboard;
mod join_graph;
mod query_builder;
pub mod schema;
mod validator;

pub use builder::run_stats_query;
pub use dashboard::{StatsFilter, StatsService};
pub use schema::discovery_json;
