use std::future::Future;
use std::time::Duration;
use actix::{Actor, Addr, AsyncContext, Context};
use tokio::task::JoinHandle;

pub trait WeakContext<A>: AsyncContext<A>
    where
        A: Actor<Context = Self>,
{
    fn run_interval_weak<F, Fut>(&mut self, dur: Duration, mut f: F) -> JoinHandle<()>
        where
            F: FnMut(Addr<A>) -> Fut + 'static,
            Fut: Future
    {
        let weak_addr = self.address().downgrade();
        actix::spawn(async move {
            while let Some(address) = weak_addr.upgrade() {
                f(address).await;
                actix::clock::sleep(dur).await;
            }
        })
    }
}

impl<A> WeakContext<A> for Context<A>
    where
        A: Actor<Context = Self>,
{

}