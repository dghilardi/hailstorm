use actix::{Actor, Addr, AsyncContext, Context};
use std::future::Future;
use std::time::Duration;
use tokio::task::JoinHandle;

/// An extension trait for `AsyncContext` that allows scheduling periodic tasks using a weak reference.
///
/// This trait provides a method to run intervals that do not keep the actor alive solely by the
/// scheduled task's existence. If the actor is stopped and all strong references are dropped,
/// the interval will cease execution.
///
/// # Type Parameters
///
/// - `A`: The actor type, which must implement `Actor` with the current context (`Self`) as its context.
pub trait WeakContext<A>: AsyncContext<A>
where
    A: Actor<Context = Self>,
{
    /// Schedules a periodic task to be run with a weak reference to the actor.
    ///
    /// This method allows for the execution of a closure at a fixed duration interval, where the closure
    /// receives a potentially upgraded `Addr<A>` if the actor is still alive. If the actor has been stopped
    /// and all strong references dropped, the interval will stop executing.
    ///
    /// # Parameters
    ///
    /// - `dur`: The `Duration` between invocations of the closure.
    /// - `f`: A closure that is called each interval with an `Addr<A>` if the actor is still alive. The
    /// closure should return a `Future`, which will be awaited before sleeping until the next interval.
    ///
    /// # Returns
    ///
    /// Returns a `JoinHandle<()>` associated with the spawned task. This handle can be used to await
    /// the task's completion, although in typical usage for periodic tasks, it may run indefinitely
    /// until the actor is dropped.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    /// use actix::{Actor, Context};
    /// use hailstorm::utils::actix::weak_context::WeakContext;
    ///
    /// struct MyActor;
    ///
    /// impl Actor for MyActor {
    ///     type Context = Context<Self>;
    ///
    ///     fn started(&mut self, ctx: &mut Self::Context) {
    ///         ctx.run_interval_weak(Duration::from_secs(5), |actor_addr| async move {
    ///             println!("Periodic task running");
    ///             // Perform operations using `actor_addr`
    ///         });
    ///     }
    /// }
    /// ```
    ///
    /// This method is particularly useful for actors that need to perform periodic maintenance tasks
    /// or polling but should not be kept alive solely by these scheduled tasks.
    fn run_interval_weak<F, Fut>(&mut self, dur: Duration, mut f: F) -> JoinHandle<()>
    where
        F: FnMut(Addr<A>) -> Fut + 'static,
        Fut: Future,
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

impl<A> WeakContext<A> for Context<A> where A: Actor<Context = Self> {}
