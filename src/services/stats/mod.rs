//! Statistics: dashboard aggregates and flexible query builder.

mod builder;
mod cache;
mod dashboard;
mod executor;
mod join_graph;
mod query_builder;
pub mod schema;
pub mod saved_queries;
mod validator;

pub use builder::run_stats_query;
pub use dashboard::{StatsFilter, StatsService};
pub use schema::discovery_json;
