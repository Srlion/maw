use std::{
    any::Any,
    future::Future,
    panic::{AssertUnwindSafe, catch_unwind},
    pin::Pin,
    task::{Context, Poll},
};

use http::StatusCode;
use pin_project_lite::pin_project;

use crate::{handler::Handler, ctx::Ctx};

pub struct NoPanicHandler;

pub struct CatchPanicMiddleware<F = NoPanicHandler> {
    on_panic: F,
}

impl CatchPanicMiddleware<NoPanicHandler> {
    pub fn new() -> Self {
        Self {
            on_panic: NoPanicHandler,
        }
    }
}

impl<F> CatchPanicMiddleware<F> {
    pub fn on_panic<H>(self, f: H) -> CatchPanicMiddleware<H> {
        CatchPanicMiddleware { on_panic: f }
    }
}

impl Handler<&mut Ctx> for CatchPanicMiddleware<NoPanicHandler> {
    type Output = ();

    async fn call(&self, c: &mut Ctx) -> Self::Output {
        if let Err(e) = CatchUnwind::new(AssertUnwindSafe(c.next())).await {
            let panic_msg = extract_panic_message(&e);
            tracing::error!(error = %panic_msg, "panic occurred");
            c.res.status(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }
}

impl<'a, F, Fut> Handler<&'a mut Ctx> for CatchPanicMiddleware<F>
where
    F: Fn(&'a mut Ctx, Box<dyn Any + Send + 'static>) -> Fut + Sync,
    Fut: Future + Send,
{
    type Output = ();

    async fn call(&self, c: &'a mut Ctx) -> Self::Output {
        if let Err(e) = CatchUnwind::new(AssertUnwindSafe(c.next())).await {
            (self.on_panic)(c, e).await;
        }
    }
}

fn extract_panic_message(e: &Box<dyn Any + Send>) -> String {
    e.downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| e.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| format!("Unknown panic type: {e:#?}"))
}

pin_project! {
    pub struct CatchUnwind<F> {
        #[pin]
        future: F,
    }
}

impl<F> CatchUnwind<F> {
    pub fn new(future: F) -> Self {
        Self { future }
    }
}

impl<F: Future + std::panic::UnwindSafe> Future for CatchUnwind<F> {
    type Output = Result<F::Output, Box<dyn Any + Send>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let f = self.project().future;
        catch_unwind(AssertUnwindSafe(|| f.poll(cx)))?.map(Ok)
    }
}
