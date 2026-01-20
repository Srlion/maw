use std::{
    any::Any,
    future::Future,
    panic::{AssertUnwindSafe, catch_unwind},
    pin::Pin,
    task::{Context, Poll},
};

use crate::{async_fn::AsyncFn1, ctx::Ctx, prelude::StatusError};

pub struct CatchPanicMiddleware {
    _p: (),
}

impl CatchPanicMiddleware {
    pub fn new() -> Self {
        Self { _p: () }
    }
}

impl Default for CatchPanicMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

impl AsyncFn1<&mut Ctx> for CatchPanicMiddleware {
    type Output = Result<(), StatusError>;

    async fn call(&self, c: &mut Ctx) -> Self::Output {
        match AssertUnwindSafe(c.next()).catch_unwind().await {
            Ok(_) => Ok(()),
            Err(e) => {
                let panic_msg = extract_panic_message(&e);
                tracing::error!(error = %panic_msg, "panic occurred");
                Err(StatusError::internal_server_error())
            }
        }
    }
}

fn extract_panic_message(e: &Box<dyn Any + Send>) -> String {
    e.downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| e.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| format!("Unknown panic type: {e:#?}"))
}

trait FutureExt: Future + std::panic::UnwindSafe + Sized {
    fn catch_unwind(self) -> CatchUnwind<Self> {
        CatchUnwind(self)
    }
}

impl<F: Future + std::panic::UnwindSafe> FutureExt for F {}

struct CatchUnwind<F>(F);

impl<F: Future + std::panic::UnwindSafe> Future for CatchUnwind<F> {
    type Output = Result<F::Output, Box<dyn Any + Send>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let f = unsafe { self.map_unchecked_mut(|s| &mut s.0) };
        catch_unwind(AssertUnwindSafe(|| f.poll(cx)))?.map(Ok)
    }
}
