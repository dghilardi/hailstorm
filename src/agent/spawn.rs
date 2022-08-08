use std::thread;
use std::thread::JoinHandle;
use actix::{Addr, System};
use crate::agent::builder::{AgentBuilder, AgentRuntime};
use crate::communication::upstream::contract::UpstreamAgentActor;
use crate::simulation::actor::simulation::SimulationActor;

pub struct AgentHandle<Upstream: UpstreamAgentActor> {
    pub handle: JoinHandle<()>,
    pub runtime: AgentRuntime<Upstream>,
}

impl<ContextBuilder, UpstreamCfg, DownstreamCfg> AgentBuilder<ContextBuilder, UpstreamCfg, DownstreamCfg>
    where
        ContextBuilder: FnOnce(Addr<SimulationActor>) -> rune::Context + Send + 'static,
        UpstreamCfg: Send + 'static,
        DownstreamCfg: Send + 'static,
{
    pub fn spawn<Upstream: UpstreamAgentActor<Config=UpstreamCfg>>(self, exit_signal: tokio::sync::oneshot::Receiver<()>) -> AgentHandle<Upstream> {
        let (snd, rcv) = std::sync::mpsc::channel();
        let handle = thread::spawn(|| System::new().block_on(async move {
            snd.send(self.launch()).expect("Error sending back AgentRuntime");
            exit_signal.await.expect("Error receiving exit_signal")
        }));
        AgentHandle {
            handle,
            runtime: rcv.recv().expect("Error receiving AgentRuntime"),
        }
    }
}