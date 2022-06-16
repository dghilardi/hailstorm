//! A distributed load testing framework inspired by [Locust](https://locust.io).
//!
//!
pub use communication::grpc;
pub use communication::server;
pub use communication::message::MultiAgentUpdateMessage;

mod communication;
pub mod agent;
pub mod simulation;
pub mod controller;