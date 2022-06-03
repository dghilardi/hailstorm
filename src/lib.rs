//! A distributed load testing framework inspired by [Locust](https://locust.io).
//!
//!
pub use communication::grpc;
pub use communication::server;

mod communication;
pub mod agent;
mod simulation;
pub mod controller;