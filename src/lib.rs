//! A distributed load testing framework inspired by [Locust](https://locust.io).
//!
//!
pub use communication::message::MultiAgentUpdateMessage;
pub use communication::protobuf::grpc;
pub use communication::server;

pub mod agent;
mod communication;
pub mod controller;
pub mod simulation;
pub mod utils;
