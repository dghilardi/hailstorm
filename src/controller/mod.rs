//! Controller module for orchestrating distributed simulations.
//!
//! The controller is the central coordination point that manages connected agents,
//! distributes simulation commands, and collects aggregated metrics.
//!
//! ## Key types
//!
//! - [`builder::ControllerBuilder`] — Fluent builder for configuring and launching a controller.
//! - [`actor::ControllerActor`] — The core actix actor managing simulation state and agent alignment.
//! - [`message`] — Messages for loading and starting simulations.
//! - [`model::simulation`] — Simulation and bot definition types.

pub mod actor;
pub mod builder;
mod client;
pub mod message;
pub mod model;
