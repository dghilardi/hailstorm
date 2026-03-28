use crate::utils::actix::synchro_interval_func::SynchroIntervalFunc;
use actix::{Actor, ActorStreamExt, AsyncContext, Context, SpawnHandle};
use std::time::Duration;

/// Extension trait for [`AsyncContext`] that provides synchronized periodic intervals.
///
/// Unlike the standard `run_interval`, this implementation aligns ticks to wall-clock
/// boundaries so that all agents fire at consistent points in time.
pub trait WeakContext<A>: AsyncContext<A>
where
    A: Actor<Context = Self>,
{
    /// Schedule a function to run at wall-clock-aligned intervals of `dur`.
    fn run_interval_synchro<F>(&mut self, dur: Duration, f: F) -> SpawnHandle
    where
        F: FnMut(&mut A, &mut A::Context) + 'static,
    {
        self.spawn(SynchroIntervalFunc::new(dur, f).finish())
    }
}

impl<A> WeakContext<A> for Context<A> where A: Actor<Context = Self> {}
