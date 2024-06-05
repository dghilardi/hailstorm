use crate::communication::message::ControllerCommandMessage;
use crate::grpc::command_item::Command;
use crate::grpc::controller_command::Target;
use crate::grpc::{AgentGroup, CommandItem, ControllerCommand, MultiAgent};
use actix::dev::RecipientRequest;
use actix::Recipient;

pub struct DownstreamClient {
    recipient: Recipient<ControllerCommandMessage>,
}

impl DownstreamClient {
    pub fn new(recipient: Recipient<ControllerCommandMessage>) -> Self {
        Self { recipient }
    }
    pub fn send_to_agent(
        &mut self,
        agent_id: u32,
        commands: Vec<Command>,
    ) -> RecipientRequest<ControllerCommandMessage> {
        self.recipient
            .send(ControllerCommandMessage(ControllerCommand {
                commands: commands
                    .into_iter()
                    .map(|cmd| CommandItem { command: Some(cmd) })
                    .collect(),
                target: Some(Target::AgentId(agent_id)),
            }))
    }

    pub fn send_to_agents(
        &mut self,
        agent_ids: Vec<u32>,
        commands: Vec<Command>,
    ) -> RecipientRequest<ControllerCommandMessage> {
        self.recipient
            .send(ControllerCommandMessage(ControllerCommand {
                commands: commands
                    .into_iter()
                    .map(|cmd| CommandItem { command: Some(cmd) })
                    .collect(),
                target: Some(Target::Agents(MultiAgent { agent_ids })),
            }))
    }

    pub fn send_broadcast(
        &self,
        commands: Vec<Command>,
    ) -> RecipientRequest<ControllerCommandMessage> {
        self.recipient
            .send(ControllerCommandMessage(ControllerCommand {
                commands: commands
                    .into_iter()
                    .map(|cmd| CommandItem { command: Some(cmd) })
                    .collect(),
                target: Some(Target::Group(AgentGroup::All.into())),
            }))
    }
}
