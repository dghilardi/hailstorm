use actix::clock::{sleep_until, Sleep};
use actix::{Actor, ActorStream};
use futures::ready;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use pin_project_lite::pin_project;
use tokio::time::Instant;

pin_project! {
    /// An `ActorStream` that periodically runs a function in the actor's context.
    ///
    /// Unless you specifically need access to the future, use [`Context::run_interval`] instead.
    ///
    /// [`Context::run_interval`]: ../prelude/trait.AsyncContext.html#method.run_interval
    ///
    /// ```
    /// # use std::io;
    /// use std::time::Duration;
    /// use actix::prelude::*;
    /// use hailstorm::utils::actix::synchro_interval_func::SynchroIntervalFunc;
    ///
    /// struct MyActor;
    ///
    /// impl MyActor {
    ///     fn tick(&mut self, context: &mut Context<Self>) {
    ///         println!("tick");
    ///     }
    /// }
    ///
    /// impl Actor for MyActor {
    ///    type Context = Context<Self>;
    ///
    ///    fn started(&mut self, context: &mut Context<Self>) {
    ///        // spawn an interval stream into our context
    ///        SynchroIntervalFunc::new(Duration::from_millis(100), Self::tick)
    ///            .finish()
    ///            .spawn(context);
    /// #      context.run_later(Duration::from_millis(200), |_, _| System::current().stop());
    ///    }
    /// }
    /// # fn main() {
    /// #    let mut sys = System::new();
    /// #    let addr = sys.block_on(async { MyActor.start() });
    /// #    sys.run();
    /// # }
    /// ```
    #[must_use = "future do nothing unless polled"]
    pub struct SynchroIntervalFunc<A: Actor> {
        f: Box<dyn FnMut(&mut A, &mut A::Context)>,
        dur: Duration,
        #[pin]
        timer: Sleep,
    }
}

impl<A: Actor> SynchroIntervalFunc<A> {
    /// Creates a new `SynchroIntervalFunc` with the given interval duration.
    pub fn new<F>(dur: Duration, f: F) -> SynchroIntervalFunc<A>
    where
        F: FnMut(&mut A, &mut A::Context) + 'static,
    {
        Self {
            f: Box::new(f),
            dur,
            timer: sleep_until(next_instant(dur)),
        }
    }
}

impl<A: Actor> ActorStream<A> for SynchroIntervalFunc<A> {
    type Item = ();

    fn poll_next(
        self: Pin<&mut Self>,
        act: &mut A,
        ctx: &mut A::Context,
        task: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        loop {
            ready!(this.timer.as_mut().poll(task));
            this.timer.as_mut().reset(next_instant(*this.dur));
            (this.f)(act, ctx);
        }
    }
}

fn next_instant(delta: Duration) -> Instant {
    let now_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis();
    let now_instant = Instant::now();

    let now_periods = now_millis / delta.as_millis();
    let next_millis = (now_periods + 1) * delta.as_millis();

    now_instant + Duration::from_millis((next_millis - now_millis) as u64)
}
