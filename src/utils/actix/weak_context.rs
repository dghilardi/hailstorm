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

#[cfg(test)]
mod tests {
    use super::*;
    use actix::prelude::*;
    use std::sync::{Arc, Mutex};
    use tokio::time::sleep;

    struct TestActor(Arc<Mutex<usize>>);

    impl Actor for TestActor {
        type Context = Context<Self>;

        fn started(&mut self, ctx: &mut Self::Context) {
            let counter_clone = self.0.clone();
            ctx.run_interval_weak(Duration::from_millis(100), move |_addr: Addr<TestActor>| {
                let counter_clone = counter_clone.clone();
                async move {
                    let mut counter = counter_clone.lock().unwrap();
                    *counter += 1;
                }
            });
        }
    }

    #[actix::test]
    async fn test_periodic_task_runs() {
        let counter = Arc::new(Mutex::new(0));
        let actor = TestActor(counter.clone()).start();

        // Wait to ensure the task has time to run a few times.
        sleep(Duration::from_millis(350)).await;

        assert!(
            *counter.lock().unwrap() >= 3,
            "Counter should have been incremented at least 3 times"
        );
    }

    #[actix::test]
    async fn test_periodic_task_stops_after_actor_dropped() {
        let counter = Arc::new(Mutex::new(0));
        let actor = TestActor(counter.clone()).start();

        // Drop the actor
        drop(actor);

        // Wait to ensure if the task would run again if it wasn't stopped.
        sleep(Duration::from_millis(350)).await;

        let final_count = *counter.lock().unwrap();
        sleep(Duration::from_millis(200)).await; // Wait more to check if the count changes

        assert_eq!(
            *counter.lock().unwrap(),
            final_count,
            "Counter should not have incremented after actor was dropped"
        );
    }
}
