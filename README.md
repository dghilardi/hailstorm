# Hailstorm
[![Test Status](https://github.com/dghilardi/hailstorm/workflows/Tests/badge.svg?event=push)](https://github.com/dghilardi/hailstorm/actions)

Hailstorm is a distributed load testing framework inspired by [Locust](https://locust.io).

## Example setup

The following example shows how a simulation can be configured and launched using the provided example biaries.
For more control over hailstorm and its features I suggest to use hailstorm as a dependency and create your own agent and controller components.

### Controller

Environment variables definition
```sh
# .env.controller

hs_address="0.0.0.0:50051"
hs_clients_distribution.hailstone="ln(1 + t/1000) * (sin(t/10) + 1) * 1000"
hs_script_path="simulation-script.rn"
```

Client behaviour can then be defined using a [rune](https://rune-rs.github.io/) script.
```rune
// simulation-script.rn

struct hailstone {
    id
}
      
impl hailstone {
    pub fn new() {
        Self { id: 10 }
    }

    pub fn register_bot(bot) {
        bot.register_action(10.0, Self::do_http_req)
    }

    pub async fn do_http_req(self) {
        let res = http::get("http://someserver:80").await;
    }
}
```

Controller can then be launched with
```sh
zenv -f .env.controller -- ./target/release/examples/controller
```

### Agent

Environment variables definition
```sh
# .env.agent
hs_max_running_bots=1000

hs_upstream.lvl1=http://localhost:50051
```

One or more agents can then be launched with
```sh
hs_address=0.0.0.0:50151 zenv -f .env.agent -- ./target/release/examples/agent
hs_address=0.0.0.0:50152 zenv -f .env.agent -- ./target/release/examples/agent
hs_address=0.0.0.0:50153 zenv -f .env.agent -- ./target/release/examples/agent
```

### Multi-level agents

Agents can also be attached to other agents, in order to have a more distributed topology. Each agent can also have more than one parent in order to reduce data loss chances.

Environment variables definition
```sh
# .env.agent.lvl2

hs_upstream.lvl1_0=http://localhost:50151
hs_upstream.lvl1_1=http://localhost:50152
hs_upstream.lvl1_2=http://localhost:50153
```

As before can be launched with
```sh
hs_address=0.0.0.0:50251 zenv -f .env.agent.lvl2 -- ./target/release/examples/agent
hs_address=0.0.0.0:50252 zenv -f .env.agent.lvl2 -- ./target/release/examples/agent
hs_address=0.0.0.0:50253 zenv -f .env.agent.lvl2 -- ./target/release/examples/agent
```