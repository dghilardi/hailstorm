use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use actix::{Actor, Addr, AsyncContext, Context, Handler, Message, MessageResponse};
use rand::{RngCore, thread_rng};
use crate::simulation::error::SimulationError;
use crate::simulation::user::registry::UserRegistry;
use crate::simulation::user_actor::{StopUser, UserActor, UserState};

struct SimulationUser {
    state: UserState,
    addr: Addr<UserActor>,
}

impl SimulationUser {
    fn stop_user(&mut self) {
        let send_outcome = self.addr.try_send(StopUser);
        if let Err(err) = send_outcome {
            log::error!("Error stopping user - {}", err);
        } else {
            self.state = UserState::Stopping;
        }
    }
}

pub struct SimulationActor {
    agent_id: u64,
    start_ts: Option<SystemTime>,
    user_registry: UserRegistry,
    agents_count: u32,
    model_shapes: HashMap<String, Box<dyn Fn(f64) -> f64>>,
    sim_users: HashMap<String, HashMap<u64, SimulationUser>>,
}

impl Actor for SimulationActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.run_interval(Duration::from_millis(1000), |act, ctx| act.tick(ctx));
    }
}

impl SimulationActor {
    pub fn new(agent_id: u64, user_registry: UserRegistry) -> Self {
        Self {
            agent_id,
            start_ts: None,
            user_registry,
            agents_count: 1,
            model_shapes: Default::default(),
            sim_users: Default::default(),
        }
    }

    pub fn register_model(&mut self, model: String, shape: String) -> Result<(), SimulationError> {
        let expr: meval::Expr = shape.parse()?;
        let shape_fun = expr.bind("t")?;

        self.model_shapes.insert(model, Box::new(shape_fun));

        Ok(())
    }

    fn tick(&mut self, ctx: &mut Context<Self>) {
        let maybe_elapsed = self.start_ts
            .as_ref()
            .filter(|start_ts| **start_ts < SystemTime::now())
            .map(SystemTime::elapsed)
            .transpose().ok().flatten() // ignore errors
            .map(|dur| dur.as_secs_f64())
            ;

        if let Some(elapsed) = maybe_elapsed {
            for (model, shape) in self.model_shapes.iter() {
                let shape_val = shape(elapsed);
                let shift = (self.agent_id % 1000) as f64 / 1000f64;
                let count = ((shape_val / self.agents_count as f64) + shift).floor() as usize;

                let users = self.sim_users.entry(model.clone())
                    .or_insert_with(Default::default);

                let running_count = users
                    .iter()
                    .filter(|(_id, u)| u.state != UserState::Stopping)
                    .count();

                users.retain(|_id, u| u.addr.connected());

                match count.cmp(&running_count) {
                    Ordering::Less => {
                        users.iter_mut()
                            .filter(|(_id, u)| u.state != UserState::Stopping)
                            .take(running_count - count)
                            .for_each(|(_id, u)| u.stop_user());
                    }
                    Ordering::Equal => {
                        // running number is as expected
                    }
                    Ordering::Greater => {
                        let mut rng = thread_rng();
                        for _idx in 0..(count - running_count) {
                            let usr_id = rng.next_u64();
                            let user_behaviour = self.user_registry
                                .build_user(model);

                            users.insert(usr_id, SimulationUser {
                                state: UserState::Running,
                                addr: UserActor::create(|_| UserActor::new(usr_id, ctx.address(), user_behaviour.expect("Model not found in registry"))),
                            });
                        }
                    }
                }
            }
        } else {
            self.sim_users.iter_mut()
                .flat_map(|(_m, u)| u.values_mut())
                .filter(|u| u.state != UserState::Stopping)
                .for_each(|u| u.stop_user());
        }
    }
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct UserStateChange {
    pub user_id: u64,
    pub state: UserState,
}

impl Handler<UserStateChange> for SimulationActor {
    type Result = ();

    fn handle(&mut self, msg: UserStateChange, _ctx: &mut Self::Context) -> Self::Result {
        let model_entry = self.sim_users.iter_mut()
            .find(|(_m, u)| u.contains_key(&msg.user_id));

        if matches!(msg.state, UserState::Stopped) {
            model_entry
                .map(|(_m, u)| u.remove(&msg.user_id));
        } else {
            let maybe_simulation_user = model_entry
                .and_then(|(_m, u)| u.get_mut(&msg.user_id));

            if let Some(u) = maybe_simulation_user {
                u.state = msg.state;
            }
        }
    }
}

pub enum SimulationCommand {
    LoadSimulation {
        model_shapes: HashMap<String, String>,
        script: String,
    },
    LaunchSimulation {
        start_ts: SystemTime,
    },
    UpdateAgentsCount {
        count: u32
    },
    StopSimulation {
        reset: bool,
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct SimulationCommandLst {
    pub commands: Vec<SimulationCommand>
}

impl Handler<SimulationCommandLst> for SimulationActor {
    type Result = ();

    fn handle(&mut self, msg: SimulationCommandLst, _ctx: &mut Self::Context) -> Self::Result {
        for cmd in msg.commands {
            match cmd {
                SimulationCommand::LoadSimulation { model_shapes, script } => {
                    let model_registration_out = model_shapes
                        .into_iter()
                        .map(|(model, shape)| self.register_model(model, shape))
                        .collect::<Result<Vec<_>, _>>();

                    if let Err(err) = model_registration_out {
                        log::error!("Error registering simulation clients - {err}")
                    }

                    let load_script_out = self.user_registry.load_script(&script);
                    if let Err(err) = load_script_out {
                        log::error!("Error loading script - {err}");
                    }
                }
                SimulationCommand::LaunchSimulation { start_ts } => {
                    self.start_ts = Some(start_ts);
                }
                SimulationCommand::UpdateAgentsCount { count } => {
                    self.agents_count = count;
                }
                SimulationCommand::StopSimulation { reset } => {
                    self.start_ts = None;
                    if reset {
                        self.user_registry.reset_script();
                        self.model_shapes.clear();
                    }
                }
            }
        }
    }
}

pub enum SimulationState {
    Idle,
    Ready,
    Waiting,
    Running,
    Stopping,
}

pub struct ClientStats {
    pub model: String,
    pub count_by_state: HashMap<UserState, usize>,
}

#[derive(MessageResponse)]
pub struct SimulationStats {
    pub stats: Vec<ClientStats>,
    pub timestamp: SystemTime,
    pub state: SimulationState,
    pub simulation_id: String,
}

#[derive(Message)]
#[rtype(result = "SimulationStats")]
pub struct FetchSimulationStats;

impl Handler<FetchSimulationStats> for SimulationActor {
    type Result = SimulationStats;

    fn handle(&mut self, _msg: FetchSimulationStats, _ctx: &mut Self::Context) -> Self::Result {
        let state = match (self.start_ts.as_ref(), self.user_registry.has_registered_models()) {
            (_, false) => SimulationState::Idle,
            (None, true) => SimulationState::Ready,
            (Some(ts), true) if *ts < SystemTime::now() => SimulationState::Running,
            (Some(_), true) => SimulationState::Waiting,
        };

        let stats = self.sim_users.iter()
            .map(|(model, usr)| ClientStats {
                model: model.clone(),
                count_by_state: count_by_state(usr),
            })
            .collect();

        SimulationStats {
            stats,
            timestamp: SystemTime::now(),
            state,
            simulation_id: "".to_string(),
        }
    }
}

fn count_by_state(usr_map: &HashMap<u64, SimulationUser>) -> HashMap<UserState, usize> {
    let mut group_by_state = HashMap::new();

    for usr in usr_map.values() {
        let entry = group_by_state.entry(usr.state)
            .or_insert(0);
        *entry += 1;
    }

    group_by_state
}