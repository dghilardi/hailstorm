//! Agent module for distributed load testing workers.
//!
//! An agent spawns and manages bot instances, executes simulation scripts, and reports metrics
//! upstream to a controller or parent agent. Agents can be organized in a multi-level hierarchy
//! for horizontal scaling.
//!
//! ## Key types
//!
//! - [`builder::AgentBuilder`] — Fluent builder for configuring and launching an agent.
//! - [`actor::AgentCoreActor`] — The core actix actor that drives the agent lifecycle.
//! - [`metrics`] — Sub-module for action timing and histogram-based metrics storage.

pub mod actor;
pub mod builder;
pub mod metrics;
pub(crate) mod spawn;
