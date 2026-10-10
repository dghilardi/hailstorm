use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use hailstorm::agent::builder::AgentBuilder;
use hailstorm::grpc::command_item::Command;
use hailstorm::grpc::hailstorm_service_server::{HailstormService, HailstormServiceServer};
use hailstorm::grpc::{
    AgentMessage, AgentSimulationState, AgentUpdate, ClientDistribution, CommandItem,
    ControllerCommand, LaunchCommand, LoadSimCommand, StopCommand,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};

type CommandSender = mpsc::Sender<Result<ControllerCommand, Status>>;

#[derive(Clone)]
struct ControllerFixture {
    joins: mpsc::Sender<(usize, CommandSender)>,
    updates: mpsc::Sender<(usize, AgentUpdate)>,
    counter: Arc<AtomicUsize>,
}

#[tonic::async_trait]
impl HailstormService for ControllerFixture {
    type JoinStream =
        Pin<Box<dyn futures::Stream<Item = Result<ControllerCommand, Status>> + Send>>;

    async fn join(
        &self,
        req: Request<Streaming<AgentMessage>>,
    ) -> Result<Response<Self::JoinStream>, Status> {
        let connection = self.counter.fetch_add(1, Ordering::SeqCst);
        let (commands, receiver) = mpsc::channel(16);
        self.joins.send((connection, commands)).await.unwrap();
        let mut input = req.into_inner();
        let updates = self.updates.clone();
        tokio::spawn(async move {
            while let Ok(Some(message)) = input.message().await {
                for update in message.updates {
                    if updates.send((connection, update)).await.is_err() {
                        return;
                    }
                }
            }
        });
        Ok(Response::new(Box::pin(ReceiverStream::new(receiver))))
    }
}

fn command(items: Vec<Command>) -> Result<ControllerCommand, Status> {
    Ok(ControllerCommand {
        target: None,
        commands: items
            .into_iter()
            .map(|command| CommandItem {
                command: Some(command),
            })
            .collect(),
    })
}

async fn wait_for_state(
    updates: &mut mpsc::Receiver<(usize, AgentUpdate)>,
    connection: usize,
    state: AgentSimulationState,
) {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let (received_connection, update) = updates.recv().await.unwrap();
            if received_connection == connection && update.state() == state {
                return;
            }
        }
    })
    .await
    .expect("agent must apply commands on the current connection");
}

#[actix::test]
async fn commands_survive_repeated_grpc_reconnections() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (join_tx, mut joins) = mpsc::channel(16);
    let (update_tx, mut updates) = mpsc::channel(32);
    let fixture = ControllerFixture {
        joins: join_tx,
        updates: update_tx,
        counter: Arc::new(AtomicUsize::new(0)),
    };
    let server = actix::spawn(
        tonic::transport::Server::builder()
            .add_service(HailstormServiceServer::new(fixture))
            .serve_with_incoming(futures::stream::unfold(listener, |listener| async {
                let incoming = listener.accept().await.map(|(stream, _)| stream);
                Some((incoming, listener))
            })),
    );
    let agent = actix::spawn(
        AgentBuilder::default()
            .agent_id(77)
            .downstream("127.0.0.1:0".parse().unwrap())
            .upstream(
                [("test".to_owned(), format!("http://{address}"))]
                    .into_iter()
                    .collect(),
            )
            .rune_context_builder(|_| rune::Context::with_default_modules().unwrap())
            .launch_grpc(),
    );

    let (first, control) = tokio::time::timeout(Duration::from_secs(5), joins.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first, 0);
    control.send(command(vec![
        Command::Load(LoadSimCommand {
            script: "struct Bot; impl Bot { pub fn new(args) { Self } pub fn register_bot(bot) { } }".into(),
            clients_evolution: vec![ClientDistribution { model: "Bot".into(), shape: "0".into() }],
        }),
        Command::Launch(LaunchCommand { start_ts: Some(SystemTime::now().into()) }),
    ])).await.unwrap();
    wait_for_state(&mut updates, first, AgentSimulationState::Running).await;
    drop(control);

    for (connection, state, item) in [
        (
            1,
            AgentSimulationState::Ready,
            Command::Stop(StopCommand { reset: false }),
        ),
        (
            2,
            AgentSimulationState::Running,
            Command::Launch(LaunchCommand {
                start_ts: Some(SystemTime::now().into()),
            }),
        ),
    ] {
        let (joined, control) = tokio::time::timeout(Duration::from_secs(5), joins.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(joined, connection);
        control.send(command(vec![item])).await.unwrap();
        wait_for_state(&mut updates, connection, state).await;
        drop(control);
    }
    agent.abort();
    server.abort();
}
