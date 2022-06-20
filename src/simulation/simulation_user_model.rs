use std::collections::HashMap;
use actix::{Actor, Addr, Context, Handler};
use crate::simulation::compound_id::U32Mask;
use crate::simulation::sequential_id_generator::SequentialIdGenerator;
use crate::simulation::simulation_actor::UserStateChange;
use crate::simulation::user::model_factory::UserModelFactory;
use crate::simulation::user_actor::{StopUser, UserActor, UserState};

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
    model_mask: U32Mask,
    id_generator: SequentialIdGenerator,
    user_factory: UserModelFactory,
    users: HashMap<u32, SimulationUser>,
}

impl SimulationUserModel {
    pub fn new(mask: U32Mask, factory: UserModelFactory) -> Self {
        Self {
            model_mask: mask,
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
        let internal_id = self.model_mask.apply_mask(usr_id);
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
        self.model_mask.matches(id) && self.users.contains_key(&id)
    }

    pub fn remove_user(&mut self, id: u32) {
        self.users.remove(&id);
        self.id_generator.release_id(self.model_mask.remove_mask(id));
    }

    pub fn get_user_mut(&mut self, id: u32) -> Option<&mut SimulationUser> {
        self.users.get_mut(&id)
    }
}