use std::{fmt::Debug, sync::Arc};

use async_trait::async_trait;

use crate::{
    async_fn::{AsyncFn2, AsyncFn3},
    next::Next,
    request::Request,
    response::Response,
    status_error::StatusError,
};

pub(crate) type HandlerOutput = Result<(), StatusError>;

pub type MiddlewareHandler = Arc<dyn MiddlewareHandlerRun>;

pub type MethodHandler = Arc<dyn MethodHandlerRun>;

pub(crate) struct HandlerWrapper<F> {
    f: F,
    #[cfg(debug_assertions)]
    name: &'static str,
    #[cfg(debug_assertions)]
    location: String,
}

impl<F> HandlerWrapper<F> {
    pub(crate) fn new(f: F, name: &'static str) -> Self {
        #[cfg(debug_assertions)]
        {
            Self {
                f,
                name,
                location: caller_location(),
            }
        }
        #[cfg(not(debug_assertions))]
        {
            Self { f }
        }
    }
}

impl<F> Debug for HandlerWrapper<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[cfg(debug_assertions)]
        {
            if self.name.is_empty() {
                write!(f, "{}", self.location)
            } else {
                write!(f, "{}: {}", self.name, self.location)
            }
        }
        #[cfg(not(debug_assertions))]
        {
            write!(f, "HandlerWrapper")
        }
    }
}

#[async_trait]
pub(crate) trait MethodHandlerRun: Send + Sync + Debug {
    async fn run(&self, req: &mut Request, res: &mut Response) -> HandlerOutput;
}

#[async_trait]
pub(crate) trait MiddlewareHandlerRun: Send + Sync + Debug {
    async fn run_next(&self, req: &mut Request, res: &mut Response, next: Next) -> HandlerOutput;
}

#[async_trait]
impl<F> MethodHandlerRun for HandlerWrapper<F>
where
    for<'a> F: AsyncFn2<&'a mut Request, &'a mut Response, Output = HandlerOutput> + Send + Sync,
{
    async fn run(&self, req: &mut Request, res: &mut Response) -> HandlerOutput {
        (self.f)(req, res).await
    }
}

#[async_trait]
impl<F> MiddlewareHandlerRun for HandlerWrapper<F>
where
    for<'a> F:
        AsyncFn3<&'a mut Request, &'a mut Response, Next, Output = HandlerOutput> + Send + Sync,
{
    async fn run_next(&self, req: &mut Request, res: &mut Response, next: Next) -> HandlerOutput {
        (self.f)(req, res, next).await
    }
}

#[cfg(debug_assertions)]
fn caller_location() -> String {
    let bt = backtrace::Backtrace::new();
    for frame in bt.frames().iter().skip(3) {
        for symbol in frame.symbols() {
            if let (Some(file), Some(line)) = (symbol.filename(), symbol.lineno()) {
                return format!("{}:{}", file.display(), line);
            }
        }
    }
    String::new()
}
