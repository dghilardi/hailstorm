//! Communication layer for inter-agent and agent-controller messaging.
//!
//! This module provides the gRPC transport, protobuf definitions, and actor wrappers
//! that enable agents and controllers to exchange commands and metric updates over the network.

mod downstream_agent_actor;
pub mod message;
pub mod notifier_actor;
pub mod protobuf;
pub mod server;
pub mod server_actor;
pub mod upstream;
