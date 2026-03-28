//! # Hailstorm
//!
//! A distributed load testing framework inspired by [Locust](https://locust.io).
//!
//! Hailstorm enables you to define bot behaviors using [Rune](https://rune-rs.github.io/) scripts,
//! orchestrate distributed agents in a multi-level topology, and collect performance metrics —
//! all with the safety and performance guarantees of Rust.
//!
//! ## Architecture
//!
//! The framework is built around three core concepts:
//!
//! - **Controller** — the entry point that manages agents and collects aggregated metrics.
//! - **Agent** — a worker that spawns and manages bot instances. Agents can connect to a
//!   controller or to other agents, forming a multi-level hierarchy for horizontal scaling.
//! - **Bot** — a single simulated user whose behavior is defined by a Rune script model.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use hailstorm::agent::builder::{AgentBuilder, SimulationParams};
//!
//! # async fn run() {
//! AgentBuilder::default()
//!     .agent_id(1)
//!     .simulation_params(
//!         SimulationParams::default()
//!             .max_running(500)
//!             .max_rate(50),
//!     )
//!     .upstream(
//!         [("ctrl".into(), "http://localhost:50051".into())]
//!             .into_iter()
//!             .collect(),
//!     )
//!     .downstream("0.0.0.0:50151".parse().unwrap())
//!     .rune_context_builder(|_sim| {
//!         rune::Context::with_default_modules().expect("default modules")
//!     })
//!     .launch_grpc()
//!     .await;
//! # }
//! ```
//!
//! ## Modules
//!
//! - [`agent`] — Agent actor, builder, metrics collection, and spawning utilities.
//! - [`controller`] — Controller actor and builder for orchestrating simulations.
//! - [`simulation`] — Bot lifecycle, Rune script integration, load shaping, and compound IDs.
//! - [`utils`] — Actix actor utilities (weak contexts, synchronized intervals) and varint encoding.

pub use communication::message::MultiAgentUpdateMessage;
pub use communication::protobuf::grpc;
pub use communication::server;
pub use communication::upstream::contract::UpstreamAgentActor;

pub mod agent;
mod communication;
pub mod controller;
pub mod simulation;
pub mod utils;
