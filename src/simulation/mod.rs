//! Simulation engine for bot lifecycle management and script execution.
//!
//! This module contains the core simulation logic including bot spawning, state management,
//! Rune script integration, mathematical load shaping, and compound ID generation.
//!
//! ## Key types
//!
//! - [`actor::simulation::SimulationActor`] — Actor managing the simulation tick loop and bot scaling.
//! - [`actor::bot::BotActor`] — Individual bot actor executing scripted actions.
//! - [`bot::registry::BotRegistry`] — Rune script loader and bot type registry.
//! - [`compound_id::CompoundId`] — Multi-level hierarchical identifier (agent/model/bot).
//! - [`shape::parse_shape_fun`] — Mathematical expression parser for load shapes.

mod bot_model;
pub(crate) mod facade;
mod sequential_id_generator;

pub mod actor;
pub mod bot;
pub mod compound_id;
pub mod error;
pub mod rune;
pub mod shape;
