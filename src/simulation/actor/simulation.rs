use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use actix::{Actor, AsyncContext, Context, Handler, Message, MessageResponse, ResponseFuture, WrapFuture};
use futures::FutureExt;
use crate::simulation::error::SimulationError;
use crate::simulation::rune::types::value::OwnedValue;
use crate::simulation::bot_model::BotModel;
use crate::simulation::bot::registry::BotRegistry;
use crate::simulation::actor::bot::{ActionExecutionError, ExecuteHandler, BotState};
use crate::utils::actix::synchro_context::WeakContext;

pub struct SimulationActor {
    agent_id: u32,
    start_ts: Option<SystemTime>,
    bot_registry: BotRegistry,
    agents_count: u32,
    model_shapes: HashMap<String, Box<dyn Fn(f64) -> f64>>,
    bots: HashMap<String, BotModel>,
}

impl Actor for SimulationActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.run_interval_synchro(Duration::from_millis(1500), |act, ctx| act.tick(ctx));
    }
}

impl SimulationActor {
    pub fn new(agent_id: u32, bot_registry: BotRegistry) -> Self {
        Self {
            agent_id,
            start_ts: None,
            bot_registry,
            agents_count: 1,
            model_shapes: Default::default(),
            bots: Default::default(),
        }
    }

    pub fn parse_shape_fun(fun: String) -> Result<impl Fn(f64) -> f64, meval::Error> {
        let mut ctx = meval::Context::new(); // built-ins
        ctx.func("rect", |x| if x.abs() > 0.5 { 0.0 } else if x.abs() == 0.5 { 0.5 } else { 1.0 })
            .func("tri", |x| if x.abs() < 1.0 { 1.0 - x.abs() } else { 0.0 })
            .func("step", |x| if x < 0.0 { 0.0 } else if x == 0.0 { 0.5 } else { 1.0 })
        ;

        let expr: meval::Expr = fun.parse()?;
        expr.bind_with_context(ctx, "t")
    }

    pub fn register_model(&mut self, model: String, shape: String) -> Result<(), SimulationError> {
        let shape_fun = Self::parse_shape_fun(shape)?;

        self.model_shapes.insert(model, Box::new(shape_fun));

        Ok(())
    }

    fn normalize_count(global_count: f64, agent_id: u32, agents_count: u32) -> usize {
        let shift = (agent_id % agents_count) as f64 / agents_count as f64;
        ((global_count / agents_count as f64) + shift).floor() as usize
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
            for (model_name, shape) in self.model_shapes.iter() {
                let shape_val = shape(elapsed);
                let count = Self::normalize_count(shape_val, self.agent_id, self.agents_count);

                let model = if let Some(mu) = self.bots.get_mut(model_name) {
                    mu
                } else {
                    log::warn!("No bot-model defined with name {model_name}");
                    break;
                };

                let running_count = model.count_active();

                model.retain(|_id, bot| bot.is_connected());

                match count.cmp(&running_count) {
                    Ordering::Less => {
                        model.bots_mut()
                            .filter(|bot| bot.state() != BotState::Stopping)
                            .take(running_count - count)
                            .for_each(|bot| bot.stop_bot());
                    }
                    Ordering::Equal => {
                        // running number is as expected
                    }
                    Ordering::Greater => {
                        let spawn_count = count - running_count;
                        for _idx in 0..spawn_count {
                            model.spawn_bot(ctx.address());
                        }
                    }
                }
            }
        } else {
            self.bots.iter_mut()
                .flat_map(|(_m, model)| model.bots_mut())
                .filter(|bot| bot.state() != BotState::Stopping)
                .for_each(|bot| bot.stop_bot());
        }
    }
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct BotStateChange {
    pub bot_id: u64,
    pub state: BotState,
}

impl Handler<BotStateChange> for SimulationActor {
    type Result = ();

    fn handle(&mut self, msg: BotStateChange, ctx: &mut Self::Context) -> Self::Result {
        let model_entry = self.bots.iter_mut()
            .find(|(_m, model)| model.contains_id(msg.bot_id));

        let entered_state = msg.state;
        if matches!(entered_state, BotState::Stopped) {
            if let Some((_m, model)) = model_entry {
                model.remove_bot(msg.bot_id);
            }
        } else {
            let maybe_bot = model_entry
                .and_then(|(_m, bot)| bot.get_bot_mut(msg.bot_id));

            if let Some(bot) = maybe_bot {
                let hook_fut = bot.change_state(entered_state)
                    .map(move |res| match res {
                        Ok(Ok(())) => {}
                        Ok(Err(err)) => log::error!("Error during hook {entered_state:?} execution - {err}"),
                        Err(mailbox_err) => log::error!("Mailbox error during hook {entered_state:?} execution - {mailbox_err}"),
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

                    let load_script_out = self.bot_registry.load_script(&script);
                    if let Err(err) = load_script_out {
                        log::error!("Error loading script - {err}");
                    }

                    self.bots.drain();
                    self.bot_registry
                        .model_names()
                        .into_iter()
                        .enumerate()
                        .for_each(|(idx, model)| {
                            self.bots.insert(model.to_string(), BotModel::new(
                                self.agent_id,
                                idx as u32,
                                self.bot_registry.build_factory(model)
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
                        self.bot_registry.reset_script();
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
    pub count_by_state: HashMap<BotState, usize>,
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
        let state = match (self.start_ts.as_ref(), self.bot_registry.has_registered_models()) {
            (_, false) => SimulationState::Idle,
            (None, true) => SimulationState::Ready,
            (Some(ts), true) if *ts < SystemTime::now() => SimulationState::Running,
            (Some(_), true) => SimulationState::Waiting,
        };

        let stats = self.bots.iter()
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
    pub bot_id: u64,
    pub execution_args: ExecuteHandler,
}

impl Handler<InvokeHandler> for SimulationActor {
    type Result = ResponseFuture<Result<OwnedValue, ActionExecutionError>>;

    fn handle(&mut self, msg: InvokeHandler, _ctx: &mut Self::Context) -> Self::Result {
        let bot_id = msg.bot_id;
        let maybe_execution_fut = self.bots.iter_mut()
            .filter_map(|(_m, model)| model.get_bot_mut(bot_id))
            .next()
            .map(|bot| bot.execute_handler(msg.execution_args.id, msg.execution_args.args));

        Box::pin(async move {
            if let Some(execution_fut) = maybe_execution_fut {
                execution_fut.await
                    .map_err(|e| ActionExecutionError::Internal(format!("Mailbox error - {e}")))?
            } else {
                Err(ActionExecutionError::Internal(format!("No bot with id {bot_id}")))
            }
        })
    }
}

#[cfg(test)]
mod test {
    use crate::simulation::actor::simulation::SimulationActor;

    #[test]
    fn test_shape_normalization() {
        let agents_count = 13;
        for n in 0..100 {
            let sum: usize = (0..agents_count).into_iter()
                .map(|agent_id| SimulationActor::normalize_count(n as f64, agent_id, agents_count))
                .sum();
            println!("{n} -> {sum}");
        }
    }
}