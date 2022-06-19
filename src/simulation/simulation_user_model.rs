use std::collections::HashMap;
use actix::{Actor, Addr, Context, Handler};
use crate::simulation::sequential_id_generator::SequentialIdGenerator;
use crate::simulation::simulation_actor::UserStateChange;
use crate::simulation::user::model_factory::UserModelFactory;
use crate::simulation::user_actor::{StopUser, UserActor, UserState};

struct SimulationUser {
    state: UserState,
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

#[derive(Default)]
pub struct SimulationUserModel {
    id_generator: SequentialIdGenerator,
    user_factory: UserModelFactory,
    users: HashMap<u32, SimulationUser>,
}

impl SimulationUserModel {
    pub fn spawn_user<A>(&mut self, addr: Addr<A>)
        where A: Actor<Context=Context<A>>
        + Handler<UserStateChange>
    {
        let usr_id = self.id_generator.next();
        let user_behaviour = self.user_factory.new_user(usr_id);

        self.users.insert(usr_id, SimulationUser {
            state: UserState::Running,
            addr: UserActor::create(|_| UserActor::new(usr_id, addr, user_behaviour)),
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

    pub fn retain<F>(&mut self, condition: F)
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
}