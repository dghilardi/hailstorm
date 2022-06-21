use std::collections::HashMap;
use actix::{Actor, Addr, Context, Handler};
use crate::simulation::compound_id::U32Mask;
use crate::simulation::sequential_id_generator::SequentialIdGenerator;
use crate::simulation::simulation_actor::UserStateChange;
use crate::simulation::user::model_factory::UserModelFactory;
use crate::simulation::user_actor::{StopUser, UserActor, UserState};
use crate::utils::varint::{VarintEncode, VarintDecode};
pub struct SimulationUser {
    pub state: UserState,
    addr: Addr<UserActor>,
}

impl SimulationUser {
    pub fn stop_user(&mut self) {
        let send_outcome = self.addr.try_send(StopUser);
        if let Err(err) = send_outcome {
            log::error!("Error stopping user - {}", err);
        } else {
            self.state = UserState::Stopping;
        }
    }

    pub fn state(&self) -> UserState {
        self.state
    }

    pub fn is_connected(&self) -> bool {
        self.addr.connected()
    }
}

pub struct SimulationUserModel {
    model_id: u32,
    id_generator: SequentialIdGenerator,
    user_factory: UserModelFactory,
    users: HashMap<u32, SimulationUser>,
}

impl SimulationUserModel {
    pub fn new(model_id: u32, factory: UserModelFactory) -> Self {
        Self {
            model_id,
            user_factory: factory,
            id_generator: Default::default(),
            users: Default::default()
        }
    }

    pub fn spawn_user<A>(&mut self, addr: Addr<A>)
        where A: Actor<Context=Context<A>>
        + Handler<UserStateChange>
    {
        let usr_id = self.id_generator.next();
        let mut internal_id_varint = vec![self.model_id, usr_id].to_varint();
        let internal_id = if internal_id_varint.len() > 4 {
            panic!("Internal Id with more than 4 bytes are not currently supported");
        } else {
            let mut prefix = vec![0u8; 4-internal_id_varint.len()];
            prefix.append(&mut internal_id_varint);
            u32::from_be_bytes(prefix.try_into().expect("Error extracting bytes"))
        };
        let user_behaviour = self.user_factory.new_user(internal_id);

        self.users.insert(internal_id, SimulationUser {
            state: UserState::Running,
            addr: UserActor::create(|_| UserActor::new(internal_id, addr, user_behaviour)),
        });
    }

    pub fn count_by_state(&self) -> HashMap<UserState, usize> {
        let mut group_by_state = HashMap::new();

        for usr in self.users.values() {
            let entry = group_by_state.entry(usr.state)
                .or_insert(0);
            *entry += 1;
        }

        group_by_state
    }

    pub fn count_active(&self) -> usize {
        self.users
            .iter()
            .filter(|(_id, u)| u.state != UserState::Stopping)
            .count()
    }

    pub fn retain<F>(&mut self, mut condition: F)
    where F: FnMut(&u32, &SimulationUser) -> bool
    {
        self.users.retain(|id, user| {
            let outcome = condition(id, user);
            if !outcome {
                self.id_generator.release_id(*id);
            }
            outcome
        })
    }

    pub fn users_mut(&mut self) -> impl Iterator<Item=&mut SimulationUser> {
        self.users.values_mut()
    }

    pub fn contains_id(&self, id: u32) -> bool {
        let sub_ids = Vec::<u32>::from_varint(&id.to_be_bytes()).expect("Error converting from varint");
        sub_ids[0] == self.model_id && self.users.contains_key(&id)
    }

    pub fn remove_user(&mut self, id: u32) {
        self.users.remove(&id);
        let sub_ids = Vec::<u32>::from_varint(&id.to_be_bytes()).expect("Error converting from varint");
        self.id_generator.release_id(sub_ids[1]);
    }

    pub fn get_user_mut(&mut self, id: u32) -> Option<&mut SimulationUser> {
        self.users.get_mut(&id)
    }
}