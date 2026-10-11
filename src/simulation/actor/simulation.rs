use std::cmp::{min, Ordering};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use crate::agent::builder::SimulationParams;
use actix::{
    Actor, AsyncContext, Context, Handler, Message, MessageResponse, ResponseFuture, WrapFuture,
};
use futures::FutureExt;

use crate::simulation::actor::bot::{ActionExecutionError, BotState, ExecuteHandler};
use crate::simulation::bot::registry::BotRegistry;
use crate::simulation::bot_model::BotModel;
use crate::simulation::rune::types::value::OwnedValue;
use crate::simulation::shape::parse_shape_fun;
use crate::utils::actix::synchro_context::WeakContext;

/// Actor representing a hailstorm simulation
pub struct SimulationActor {
    agent_id: u32,
    simulation_params: SimulationParams,
    start_ts: Option<SystemTime>,
    bot_registry: BotRegistry,
    agents_count: u32,
    agent_index: Option<u32>,
    model_shapes: HashMap<String, Box<dyn Fn(f64) -> f64>>,
    bots: HashMap<String, BotModel>,
    simulation_id: String,
    simulation_error: Option<String>,
}

impl Actor for SimulationActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.run_interval_synchro(Duration::from_millis(1500), |act, ctx| act.tick(ctx));
    }
}

impl SimulationActor {
    pub(crate) fn new(
        agent_id: u32,
        simulation_params: SimulationParams,
        bot_registry: BotRegistry,
    ) -> Self {
        Self {
            agent_id,
            simulation_params,
            start_ts: None,
            bot_registry,
            agents_count: 1,
            agent_index: Some(0),
            model_shapes: Default::default(),
            bots: Default::default(),
            simulation_id: String::new(),
            simulation_error: None,
        }
    }

    /// Load a simulation atomically: either every shape and bot model is valid, or nothing is kept.
    fn load_simulation(
        &mut self,
        model_shapes: HashMap<String, String>,
        script: &str,
    ) -> Result<(), String> {
        let mut shapes: HashMap<String, Box<dyn Fn(f64) -> f64>> = HashMap::new();
        for (model, shape) in model_shapes {
            let shape_fun = parse_shape_fun(shape)
                .map_err(|err| format!("Invalid shape for model '{model}' - {err}"))?;
            shapes.insert(model, Box::new(shape_fun));
        }

        self.bot_registry
            .load_script(script)
            .map_err(|err| err.to_string())?;

        let bot_models = self.bot_registry.model_names();
        let mut missing = shapes
            .keys()
            .filter(|model| !bot_models.contains(model))
            .cloned()
            .collect::<Vec<_>>();
        missing.sort();
        if let Some(model) = missing.first() {
            return Err(format!("No bot type named '{model}' in the script"));
        }

        self.bots.clear();
        for (idx, model) in bot_models.into_iter().enumerate() {
            if let Some(factory) = self.bot_registry.build_factory(model) {
                self.bots.insert(
                    model.to_string(),
                    BotModel::new(self.agent_id, idx as u32, factory)
                        .with_execution_limits(self.simulation_params.execution_limits),
                );
            }
        }
        self.model_shapes = shapes;
        Ok(())
    }

    fn reset(&mut self) {
        self.start_ts = None;
        self.bot_registry.reset_script();
        self.model_shapes.clear();
        self.bots.clear();
    }

    fn normalize_count(global_count: f64, agent_index: u32, agents_count: u32) -> usize {
        if !global_count.is_finite()
            || global_count < 0.0
            || agents_count == 0
            || agent_index >= agents_count
        {
            return 0;
        }
        let count = global_count.floor() as usize;
        let agents_count = agents_count as usize;
        count / agents_count + usize::from((agent_index as usize) < count % agents_count)
    }

    fn update_agents(&mut self, mut agent_ids: Vec<u32>) {
        agent_ids.sort_unstable();
        agent_ids.dedup();
        self.agents_count = agent_ids.len() as u32;
        self.agent_index = agent_ids
            .binary_search(&self.agent_id)
            .ok()
            .map(|index| index as u32);
    }

    fn tick(&mut self, ctx: &mut Context<Self>) {
        let maybe_elapsed = self
            .start_ts
            .as_ref()
            .filter(|start_ts| **start_ts < SystemTime::now())
            .map(SystemTime::elapsed)
            .transpose()
            .ok()
            .flatten() // ignore errors
            .map(|dur| dur.as_secs_f64());

        if let Some(elapsed) = maybe_elapsed {
            for (model_name, shape) in self.model_shapes.iter() {
                let shape_val = shape(elapsed);
                let count = match (
                    self.agent_index
                        .map(|index| Self::normalize_count(shape_val, index, self.agents_count))
                        .unwrap_or(0),
                    self.simulation_params.max_running,
                ) {
                    (normalized_count, Some(max_running)) => min(normalized_count, max_running),
                    (normalized_count, None) => normalized_count,
                };

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
                        model
                            .bots_mut()
                            .filter(|bot| bot.state() != BotState::Stopping)
                            .take(running_count - count)
                            .for_each(|bot| bot.stop_bot());
                    }
                    Ordering::Equal => {
                        // running number is as expected
                    }
                    Ordering::Greater => {
                        let spawn_count =
                            match (count - running_count, self.simulation_params.max_rate) {
                                (running_diff, Some(max_rate)) => min(running_diff, max_rate),
                                (running_diff, None) => running_diff,
                            };

                        for _idx in 0..spawn_count {
                            model.spawn_bot(ctx.address());
                        }
                    }
                }
            }
        } else {
            self.bots
                .iter_mut()
                .flat_map(|(_m, model)| model.bots_mut())
                .filter(|bot| bot.state() != BotState::Stopping)
                .for_each(|bot| bot.stop_bot());
        }
    }
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
/// Notify that a specific bot changed its state
pub struct BotStateChange {
    pub bot_id: u64,
    pub state: BotState,
}

impl Handler<BotStateChange> for SimulationActor {
    type Result = ();

    fn handle(&mut self, msg: BotStateChange, ctx: &mut Self::Context) -> Self::Result {
        let model_entry = self
            .bots
            .iter_mut()
            .find(|(_m, model)| model.contains_id(msg.bot_id));

        let entered_state = msg.state;
        if matches!(entered_state, BotState::Stopped) {
            if let Some((_m, model)) = model_entry {
                model.remove_bot(msg.bot_id);
            }
        } else {
            let maybe_bot = model_entry.and_then(|(_m, bot)| bot.get_bot_mut(msg.bot_id));

            if let Some(bot) = maybe_bot {
                let hook_fut = bot.change_state(entered_state).map(move |res| match res {
                    Ok(Ok(())) => {}
                    Ok(Err(err)) => {
                        log::error!("Error during hook {entered_state:?} execution - {err}")
                    }
                    Err(mailbox_err) => log::error!(
                        "Mailbox error during hook {entered_state:?} execution - {mailbox_err}"
                    ),
                });
                ctx.spawn(hook_fut.into_actor(self));
            }
        }
    }
}

/// Commands that can be sent to the simulation actor to control its lifecycle.
pub(crate) enum SimulationCommand {
    /// Load a simulation with given model shapes and Rune script.
    LoadSimulation {
        simulation_id: String,
        model_shapes: HashMap<String, String>,
        script: String,
    },
    /// Launch the loaded simulation at the specified start time.
    LaunchSimulation { start_ts: SystemTime },
    /// Update the total number of agents participating in the simulation.
    UpdateAgentsCount { count: u32 },
    /// Update the complete membership used to distribute bot counts.
    UpdateAgents { agent_ids: Vec<u32> },
    /// Stop the simulation, optionally resetting all state.
    StopSimulation { reset: bool },
}

#[derive(Message, Default)]
#[rtype(result = "()")]
/// Control the simulation sending a list of command
pub(crate) struct SimulationCommandLst {
    pub commands: Vec<SimulationCommand>,
}

impl Handler<SimulationCommandLst> for SimulationActor {
    type Result = ();

    fn handle(&mut self, msg: SimulationCommandLst, _ctx: &mut Self::Context) -> Self::Result {
        for cmd in msg.commands {
            match cmd {
                SimulationCommand::LoadSimulation {
                    simulation_id,
                    model_shapes,
                    script,
                } => {
                    // A non-empty identifier names one immutable definition. Heartbeats can
                    // cause the controller to resend it after it has already been applied.
                    if !simulation_id.is_empty() && simulation_id == self.simulation_id {
                        continue;
                    }
                    self.start_ts = None;
                    self.simulation_error = self
                        .load_simulation(model_shapes, &script)
                        .err()
                        .inspect(|err| {
                            log::error!("Error loading simulation '{simulation_id}' - {err}")
                        });
                    if self.simulation_error.is_some() {
                        self.reset();
                    }
                    self.simulation_id = simulation_id;
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
                    // Compatibility with controllers that only send a count. Modern controllers
                    // follow this command with the complete membership, replacing this fallback.
                    self.agent_index = Some(self.agent_id % self.agents_count);
                }
                SimulationCommand::UpdateAgents { agent_ids } => {
                    self.update_agents(agent_ids);
                }
                SimulationCommand::StopSimulation { reset } => {
                    self.start_ts = None;
                    self.bots
                        .values_mut()
                        .flat_map(BotModel::bots_mut)
                        .for_each(|bot| bot.stop_bot());
                    if reset {
                        self.bot_registry.reset_script();
                        self.model_shapes.clear();
                        self.simulation_id.clear();
                        self.simulation_error = None;
                    }
                }
            }
        }
    }
}

/// Current state of the simulation from the agent's perspective.
pub(crate) enum SimulationState {
    Idle,
    Ready,
    Waiting,
    Running,
    Stopping,
}

pub(crate) struct ClientStats {
    pub model: String,
    pub timestamp: SystemTime,
    pub count_by_state: HashMap<BotState, usize>,
}

#[derive(MessageResponse)]
pub(crate) struct SimulationStats {
    pub stats: Vec<ClientStats>,
    pub timestamp: SystemTime,
    pub state: SimulationState,
    pub simulation_id: String,
    pub simulation_error: Option<String>,
}

#[derive(Message)]
#[rtype(result = "SimulationStats")]
pub(crate) struct FetchSimulationStats;

impl Handler<FetchSimulationStats> for SimulationActor {
    type Result = SimulationStats;

    fn handle(&mut self, _msg: FetchSimulationStats, _ctx: &mut Self::Context) -> Self::Result {
        let state = match (
            self.start_ts.as_ref(),
            self.bot_registry.has_registered_models(),
        ) {
            (None, _) if self.bots.values().any(BotModel::has_bots) => SimulationState::Stopping,
            (_, false) => SimulationState::Idle,
            (None, true) => SimulationState::Ready,
            (Some(ts), true) if *ts < SystemTime::now() => SimulationState::Running,
            (Some(_), true) => SimulationState::Waiting,
        };

        let stats = self
            .bots
            .iter()
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
            simulation_id: self.simulation_id.clone(),
            simulation_error: self.simulation_error.clone(),
        }
    }
}

#[derive(Message)]
#[rtype(result = "Result<OwnedValue, ActionExecutionError>")]
/// Invoke a handler with args on a specific bot
pub struct InvokeHandler {
    pub(crate) bot_id: u64,
    pub(crate) execution_args: ExecuteHandler,
}

impl InvokeHandler {
    pub fn new(bot_id: u64, args: ExecuteHandler) -> Self {
        Self {
            bot_id,
            execution_args: args,
        }
    }
}

impl Handler<InvokeHandler> for SimulationActor {
    type Result = ResponseFuture<Result<OwnedValue, ActionExecutionError>>;

    fn handle(&mut self, msg: InvokeHandler, _ctx: &mut Self::Context) -> Self::Result {
        let bot_id = msg.bot_id;
        let maybe_execution_fut = self
            .bots
            .iter_mut()
            .filter_map(|(_m, model)| model.get_bot_mut(bot_id))
            .next()
            .map(|bot| bot.execute_handler(msg.execution_args.id, msg.execution_args.args));

        Box::pin(async move {
            if let Some(execution_fut) = maybe_execution_fut {
                execution_fut
                    .await
                    .map_err(|e| ActionExecutionError::Internal(format!("Mailbox error - {e}")))?
            } else {
                Err(ActionExecutionError::Internal(format!(
                    "No bot with id {bot_id}"
                )))
            }
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::agent::metrics::manager::actor::MetricsManagerActor;
    use crate::simulation::actor::simulation::SimulationActor;
    use actix::Actor;

    fn simulation(agent_id: u32) -> SimulationActor {
        let registry = BotRegistry::new(
            rune::Context::with_default_modules().unwrap(),
            MetricsManagerActor::start_default(),
        )
        .unwrap();
        SimulationActor::new(agent_id, SimulationParams::default(), registry)
    }

    const BOT_SCRIPT: &str = r#"
        struct Bot;
        impl Bot {
            pub fn new(args) { Self }
            pub fn register_bot(bot) {
                bot.register_action(hailstorm::bot::alive(1.0), Self::action);
            }
            pub fn action(self) {}
        }
    "#;

    fn load(id: &str, shapes: &[(&str, &str)], script: &str) -> SimulationCommand {
        SimulationCommand::LoadSimulation {
            simulation_id: id.to_string(),
            model_shapes: shapes
                .iter()
                .map(|(model, shape)| (model.to_string(), shape.to_string()))
                .collect(),
            script: script.to_string(),
        }
    }

    async fn stats_after(
        addr: &actix::Addr<SimulationActor>,
        commands: Vec<SimulationCommand>,
    ) -> SimulationStats {
        addr.send(SimulationCommandLst { commands }).await.unwrap();
        addr.send(FetchSimulationStats).await.unwrap()
    }

    #[actix::test]
    async fn a_valid_load_reports_its_identifier_without_errors() {
        let addr = simulation(0).start();
        let stats = stats_after(&addr, vec![load("run-1", &[("Bot", "10")], BOT_SCRIPT)]).await;
        assert!(matches!(stats.state, SimulationState::Ready));
        assert_eq!(stats.simulation_id, "run-1");
        assert_eq!(stats.simulation_error, None);
    }

    #[actix::test]
    async fn repeated_loads_preserve_running_bots_and_their_start_time() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let created = Arc::new(AtomicUsize::new(0));
        let stopped = Arc::new(AtomicUsize::new(0));
        let mut context = rune::Context::with_default_modules().unwrap();
        let mut module = rune::Module::with_crate("test").unwrap();
        let counter = created.clone();
        module
            .function("created", move || {
                counter.fetch_add(1, Ordering::SeqCst);
            })
            .build()
            .unwrap();
        let counter = stopped.clone();
        module
            .function("stopped", move || {
                counter.fetch_add(1, Ordering::SeqCst);
            })
            .build()
            .unwrap();
        context.install(module).unwrap();
        let registry = BotRegistry::new(context, MetricsManagerActor::start_default()).unwrap();
        let addr = SimulationActor::new(0, SimulationParams::default(), registry).start();
        let script = r#"
            struct Bot;
            impl Bot {
                pub fn new(args) { test::created(); Self }
                pub fn register_bot(bot) {
                    bot.register_action(hailstorm::bot::enter_state(hailstorm::bot::BotState::Stopping), Self::stop);
                }
                pub fn stop(self) { test::stopped(); }
            }
        "#;
        let start_ts = SystemTime::now() - Duration::from_secs(1);
        stats_after(
            &addr,
            vec![
                load("run-1", &[("Bot", "1")], script),
                SimulationCommand::LaunchSimulation { start_ts },
            ],
        )
        .await;
        tokio::time::timeout(Duration::from_secs(5), async {
            while created.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();

        let stats = stats_after(&addr, vec![load("run-1", &[("Bot", "1")], script)]).await;
        assert!(matches!(stats.state, SimulationState::Running));
        assert_eq!(
            stats.stats[0].count_by_state.get(&BotState::Running),
            Some(&1)
        );
        tokio::time::sleep(Duration::from_millis(1600)).await;
        assert_eq!(created.load(Ordering::SeqCst), 1);
        assert_eq!(stopped.load(Ordering::SeqCst), 0);
        stats_after(
            &addr,
            vec![SimulationCommand::StopSimulation { reset: true }],
        )
        .await;
    }

    #[actix::test]
    async fn a_new_definition_does_not_inherit_the_previous_start_time() {
        let addr = simulation(0).start();
        stats_after(
            &addr,
            vec![
                load("run-1", &[("Bot", "0")], BOT_SCRIPT),
                SimulationCommand::LaunchSimulation {
                    start_ts: SystemTime::now(),
                },
            ],
        )
        .await;
        let stats = stats_after(&addr, vec![load("run-2", &[("Bot", "0")], BOT_SCRIPT)]).await;
        assert!(matches!(stats.state, SimulationState::Ready));
        assert_eq!(stats.simulation_id, "run-2");
    }

    #[actix::test]
    async fn duplicate_failed_loads_preserve_the_error_until_reset() {
        let addr = simulation(0).start();
        let failed = stats_after(&addr, vec![load("run-1", &[("Bot", "10 *")], BOT_SCRIPT)]).await;
        let duplicate = stats_after(&addr, vec![load("run-1", &[("Bot", "0")], BOT_SCRIPT)]).await;
        assert!(matches!(duplicate.state, SimulationState::Idle));
        assert_eq!(duplicate.simulation_error, failed.simulation_error);
        stats_after(
            &addr,
            vec![SimulationCommand::StopSimulation { reset: true }],
        )
        .await;
        let retried = stats_after(&addr, vec![load("run-1", &[("Bot", "0")], BOT_SCRIPT)]).await;
        assert!(matches!(retried.state, SimulationState::Ready));
    }

    #[actix::test]
    async fn legacy_loads_without_identifiers_are_always_applied() {
        let addr = simulation(0).start();
        stats_after(&addr, vec![load("", &[("Bot", "0")], BOT_SCRIPT)]).await;
        let stats = stats_after(&addr, vec![load("", &[("Bot", "10 *")], BOT_SCRIPT)]).await;
        assert!(matches!(stats.state, SimulationState::Idle));
        assert!(stats.simulation_error.is_some());
    }

    #[actix::test]
    async fn invalid_simulations_are_reported_and_leave_the_agent_idle() {
        for (shapes, script, expected) in [
            (
                vec![("Bot", "10 *")],
                BOT_SCRIPT,
                "Invalid shape for model 'Bot'",
            ),
            (
                vec![("Other", "10")],
                BOT_SCRIPT,
                "No bot type named 'Other'",
            ),
            (vec![("Bot", "10")], "struct Bot", "Invalid script"),
        ] {
            let addr = simulation(0).start();
            stats_after(&addr, vec![load("run-1", &[("Bot", "10")], BOT_SCRIPT)]).await;
            let stats = stats_after(&addr, vec![load("run-2", &shapes, script)]).await;
            assert!(matches!(stats.state, SimulationState::Idle));
            assert_eq!(stats.simulation_id, "run-2");
            let error = stats.simulation_error.expect("the error must be reported");
            assert!(error.contains(expected), "{error}");
        }
    }

    #[actix::test]
    async fn a_reset_clears_the_reported_simulation() {
        let addr = simulation(0).start();
        stats_after(&addr, vec![load("run-1", &[("Bot", "10 *")], BOT_SCRIPT)]).await;
        let stats = stats_after(
            &addr,
            vec![SimulationCommand::StopSimulation { reset: true }],
        )
        .await;
        assert_eq!(stats.simulation_id, "");
        assert_eq!(stats.simulation_error, None);
    }

    #[actix::test]
    async fn arbitrary_ids_receive_unique_indices_and_preserve_total_load() {
        for ids in [vec![0, 2], vec![1, 3], vec![77, 1000, u32::MAX]] {
            let mut total = 0;
            for (index, id) in ids.iter().enumerate() {
                let mut simulation = simulation(*id);
                simulation.update_agents(ids.iter().rev().copied().collect());
                assert_eq!(simulation.agent_index, Some(index as u32));
                total += SimulationActor::normalize_count(
                    7.0,
                    simulation.agent_index.unwrap(),
                    simulation.agents_count,
                );
            }
            assert_eq!(total, 7);
        }
    }

    #[actix::test]
    async fn membership_deduplicates_ids_and_excludes_removed_agents() {
        let mut simulation = simulation(3);
        simulation.update_agents(vec![3, 1, 3]);
        assert_eq!(simulation.agents_count, 2);
        assert_eq!(simulation.agent_index, Some(1));
        simulation.update_agents(vec![1, 5]);
        assert_eq!(simulation.agent_index, None);
        simulation.update_agents(vec![]);
        assert_eq!(simulation.agents_count, 0);
        assert_eq!(simulation.agent_index, None);
    }

    #[test]
    fn normalize_count_distributes_evenly() {
        for agents_count in 1..=16 {
            for n in 0..100 {
                let counts = (0..agents_count)
                    .map(|index| {
                        SimulationActor::normalize_count(n as f64 + 0.75, index, agents_count)
                    })
                    .collect::<Vec<_>>();
                assert_eq!(counts.iter().sum::<usize>(), n);
                assert!(counts.iter().max().unwrap() - counts.iter().min().unwrap() <= 1);
            }
        }
    }

    #[test]
    fn normalization_assigns_the_remainder_to_distinct_indices() {
        assert_eq!(SimulationActor::normalize_count(3.0, 0, 2), 2);
        assert_eq!(SimulationActor::normalize_count(3.0, 1, 2), 1);
    }

    #[test]
    fn invalid_shapes_or_assignments_do_not_spawn_bots() {
        for count in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(SimulationActor::normalize_count(count, 0, 1), 0);
        }
        assert_eq!(SimulationActor::normalize_count(100.0, 0, 0), 0);
        assert_eq!(SimulationActor::normalize_count(100.0, 2, 2), 0);
    }

    #[test]
    fn normalize_count_single_agent() {
        assert_eq!(SimulationActor::normalize_count(10.0, 0, 1), 10);
        assert_eq!(SimulationActor::normalize_count(0.0, 0, 1), 0);
    }

    #[test]
    fn normalize_count_zero_bots() {
        for agent_index in 0..5 {
            assert_eq!(SimulationActor::normalize_count(0.0, agent_index, 5), 0);
        }
    }
}
