use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use actix::{Actor, AsyncContext, Context, Handler, Message, MessageResponse, ResponseFuture, WrapFuture};
use futures::FutureExt;
use crate::simulation::error::SimulationError;
use crate::simulation::rune::types::value::OwnedValue;
use crate::simulation::simulation_user_model::SimulationUserModel;
use crate::simulation::user::registry::UserRegistry;
use crate::simulation::user_actor::{ActionExecutionError, ExecuteHandler, UserState};

pub struct SimulationActor {
    agent_id: u64,
    start_ts: Option<SystemTime>,
    user_registry: UserRegistry,
    agents_count: u32,
    model_shapes: HashMap<String, Box<dyn Fn(f64) -> f64>>,
    sim_users: HashMap<String, SimulationUserModel>,
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
        let mut ctx = meval::Context::new(); // built-ins
        ctx.func("rect", |x| if x.abs() > 0.5 { 0.0 } else if x.abs() == 0.5 { 0.5 } else { 1.0 })
            .func("tri", |x| if x.abs() < 1.0 { 1.0 - x.abs() } else { 0.0 })
            .func("step", |x| if x < 0.0 { 0.0 } else if x == 0.0 { 0.5 } else { 1.0 })
            ;

        let expr: meval::Expr = shape.parse()?;
        let shape_fun = expr.bind_with_context(ctx, "t")?;

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

                let model_users = if let Some(mu) = self.sim_users.get_mut(model) {
                    mu
                } else {
                    log::warn!("No simulation-user-model defined for model {model}");
                    break;
                };

                let running_count = model_users.count_active();

                model_users.retain(|_id, u| u.is_connected());

                match count.cmp(&running_count) {
                    Ordering::Less => {
                        model_users.users_mut()
                            .filter(|u| u.state() != UserState::Stopping)
                            .take(running_count - count)
                            .for_each(|u| u.stop_user());
                    }
                    Ordering::Equal => {
                        // running number is as expected
                    }
                    Ordering::Greater => {
                        let spawn_count = count - running_count;
                        for _idx in 0..spawn_count {
                            model_users.spawn_user(ctx.address());
                        }
                    }
                }
            }
        } else {
            self.sim_users.iter_mut()
                .flat_map(|(_m, u)| u.users_mut())
                .filter(|u| u.state() != UserState::Stopping)
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

    fn handle(&mut self, msg: UserStateChange, ctx: &mut Self::Context) -> Self::Result {
        let model_entry = self.sim_users.iter_mut()
            .find(|(_m, u)| u.contains_id(msg.user_id));

        if matches!(msg.state, UserState::Stopped) {
            if let Some((_m, u)) = model_entry {
                u.remove_user(msg.user_id);
            }
        } else {
            let maybe_simulation_user = model_entry
                .and_then(|(_m, u)| u.get_user_mut(msg.user_id));

            if let Some(u) = maybe_simulation_user {
                u.state = msg.state;
                let hook_fut = u.trigger_hook(msg.state)
                    .map(|res| match res {
                        Ok(Ok(())) => {}
                        Ok(Err(err)) => log::error!("Error during hook execution - {err}"),
                        Err(mailbox_err) => log::error!("Mailbox error during hook execution - {mailbox_err}"),
                    });
                ctx.spawn(hook_fut.into_actor(self));
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
    },
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct SimulationCommandLst {
    pub commands: Vec<SimulationCommand>,
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

                    self.sim_users.drain();
                    self.user_registry
                        .model_names()
                        .into_iter()
                        .enumerate()
                        .for_each(|(idx, model)| {
                            self.sim_users.insert(model.to_string(), SimulationUserModel::new(
                                idx as u32,
                                self.user_registry.build_factory(model)
                                    .unwrap_or_else(|| panic!("No factory for {model}")),
                            ));
                        });
                }
                SimulationCommand::LaunchSimulation { start_ts } => {
                    self.start_ts = Some(start_ts);
                }
                SimulationCommand::UpdateAgentsCount { count } => {
                    if count > 0 {
                        self.agents_count = count;
                    } else {
                        log::warn!("Received agents_count = 0. setting it to 1");
                        self.agents_count = 1;
                    }
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
    pub timestamp: SystemTime,
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
                timestamp: SystemTime::now(),
                model: model.clone(),
                count_by_state: usr.count_by_state(),
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

#[derive(Message)]
#[rtype(result = "Result<OwnedValue, ActionExecutionError>")]
pub struct InvokeHandler {
    pub user_id: u64,
    pub execution_args: ExecuteHandler,
}

impl Handler<InvokeHandler> for SimulationActor {
    type Result = ResponseFuture<Result<OwnedValue, ActionExecutionError>>;

    fn handle(&mut self, msg: InvokeHandler, _ctx: &mut Self::Context) -> Self::Result {
        let user_id = msg.user_id;
        let maybe_execution_fut = self.sim_users.iter_mut()
            .filter_map(|(_m, u)| u.get_user_mut(user_id))
            .next()
            .map(|user| user.execute_handler(msg.execution_args.id, msg.execution_args.args));

        Box::pin(async move {
            if let Some(execution_fut) = maybe_execution_fut {
                execution_fut.await
                    .map_err(|e| ActionExecutionError::Internal(format!("Mailbox error - {e}")))?
            } else {
                Err(ActionExecutionError::Internal(format!("No user with id {user_id}")))
            }
        })
    }
}