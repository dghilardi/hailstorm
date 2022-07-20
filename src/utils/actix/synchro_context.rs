use std::time::Duration;
use actix::{Actor, ActorStreamExt, AsyncContext, Context, SpawnHandle};
use crate::utils::actix::synchro_interval_func::SynchroIntervalFunc;

pub trait WeakContext<A>: AsyncContext<A>
    where
        A: Actor<Context = Self>,
{
    fn run_interval_synchro<F>(&mut self, dur: Duration, f: F) -> SpawnHandle
        where
            F: FnMut(&mut A, &mut A::Context) + 'static,
    {
        self.spawn(SynchroIntervalFunc::new(dur, f).finish())
    }
}

impl<A> WeakContext<A> for Context<A>
    where
        A: Actor<Context = Self>,
{

}