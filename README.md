# Hailstorm

[![Test Status](https://github.com/dghilardi/hailstorm/workflows/Tests/badge.svg?event=push)](https://github.com/dghilardi/hailstorm/actions)
[![Crates.io](https://img.shields.io/crates/v/hailstorm.svg)](https://crates.io/crates/hailstorm)
[![Documentation](https://docs.rs/hailstorm/badge.svg)](https://docs.rs/hailstorm)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A distributed load testing framework for Rust, inspired by [Locust](https://locust.io).

Define bot behaviors with [Rune](https://rune-rs.github.io/) scripts, scale horizontally with a multi-level agent topology, and collect detailed performance metrics — all with the safety and performance guarantees of Rust.

## Features

- **Scriptable bots** — Write bot behaviors in Rune, a dynamic language embedded in Rust. No recompilation needed to change test scenarios.
- **Distributed architecture** — Controllers orchestrate agents, which can form multi-level hierarchies for massive horizontal scaling.
- **Dynamic load shaping** — Define load curves with mathematical expressions (e.g., `ln(1 + t/1000) * sin(t/10) * 1000`).
- **Built-in metrics** — Histogram-based latency tracking with per-action, per-outcome breakdowns.
- **Pluggable storage** — Initialize bot state from CSV files or custom data sources.
- **gRPC transport** — Efficient binary communication between all components.

## Architecture

```
┌────────────┐     gRPC      ┌──────────┐     gRPC      ┌──────────┐
│ Controller │◄──────────────►│ Agent L1 │◄──────────────►│ Agent L2 │
│            │                │          │                │          │
│ • Metrics  │                │ • Bots   │                │ • Bots   │
│ • Commands │                │ • Metrics│                │ • Metrics│
└────────────┘                └──────────┘                └──────────┘
```

- **Controller** — Central coordination point. Distributes simulation commands and collects aggregated metrics.
- **Agent** — Worker process that spawns and manages bots. Agents can connect to the controller or to other agents.
- **Bot** — A simulated user whose behavior is defined by a Rune script model.

## Quick start

### Prerequisites

- Rust 1.60+ with Cargo
- Protocol Buffers compiler (`protoc`)

### Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
hailstorm = "0.2"
```

### Bot script

Bot behaviors are defined using [Rune](https://rune-rs.github.io/) scripts:

```rune
// simulation-script.rn

struct Hailstone {
    id
}

impl Hailstone {
    pub fn new(params) {
        Self { id: params.bot_id }
    }

    pub fn register_bot(bot) {
        bot.register_action(10.0, Self::do_http_req);
    }

    pub async fn do_http_req(self) {
        let response = http::get("http://target-server:80/api/health").await;
    }
}
```

### Controller setup

```sh
# .env.controller
hs_address="0.0.0.0:50051"
hs_clients_distribution.hailstone="ln(1 + t/1000) * (sin(t/10) + 1) * 1000"
hs_script_path="simulation-script.rn"
```

```sh
zenv -f .env.controller -- ./target/release/examples/controller
```

### Agent setup

```sh
# .env.agent
hs_upstream.lvl1=http://localhost:50051
hs_simulation.running_max=1000
hs_simulation.rate_max=100
```

Launch one or more agents:

```sh
hs_address=0.0.0.0:50151 zenv -f .env.agent -- ./target/release/examples/agent
hs_address=0.0.0.0:50152 zenv -f .env.agent -- ./target/release/examples/agent
hs_address=0.0.0.0:50153 zenv -f .env.agent -- ./target/release/examples/agent
```

### Multi-level topology

Agents can connect to other agents instead of the controller directly, enabling deeper distribution:

```sh
# .env.agent.lvl2
hs_upstream.lvl1_0=http://localhost:50151
hs_upstream.lvl1_1=http://localhost:50152
hs_upstream.lvl1_2=http://localhost:50153
```

```sh
hs_address=0.0.0.0:50251 zenv -f .env.agent.lvl2 -- ./target/release/examples/agent
```

## Load shape functions

Load shapes are mathematical expressions evaluated over time (`t` in seconds). Built-in shape primitives:

| Function | Description |
|---|---|
| `rect(t)` | Rectangular pulse: 1.0 for \|t\| < 0.5 |
| `tri(t)` | Triangular pulse: 1.0 - \|t\| for \|t\| < 1.0 |
| `step(t)` | Unit step: 1.0 for t > 0 |
| `trapz(t, b_low, b_sup)` | Trapezoidal pulse with configurable bounds |
| `costrapz(t, b_low, b_sup)` | Cosine-tapered trapezoidal pulse |

Standard math functions (`sin`, `cos`, `ln`, `exp`, etc.) are also available.

## Using as a library

For full control, use hailstorm as a dependency and build custom agents and controllers:

```rust,no_run
use hailstorm::agent::builder::{AgentBuilder, SimulationParams};
use hailstorm::simulation::rune::extension::{env, storage};

#[actix::main]
async fn main() {
    AgentBuilder::default()
        .agent_id(1)
        .simulation_params(
            SimulationParams::default()
                .max_running(500)
                .max_rate(50),
        )
        .upstream(
            [("ctrl".into(), "http://localhost:50051".into())]
                .into_iter()
                .collect(),
        )
        .downstream("0.0.0.0:50151".parse().unwrap())
        .rune_context_builder(|_sim| {
            let mut ctx = rune::Context::with_default_modules()
                .expect("default modules");
            ctx.install(
                env::module(env::EnvModuleConf::default()).unwrap()
            ).unwrap();
            ctx
        })
        .launch_grpc()
        .await
        .expect("agent failed");
}
```

## Contributing

Contributions are welcome! Please open an issue or submit a pull request.

## License

This project is licensed under the [MIT License](LICENSE).
