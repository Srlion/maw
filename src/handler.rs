use std::{fmt::Debug, sync::Arc};

use async_trait::async_trait;
use http::Method;

use crate::{async_fn::AsyncFn1, ctx::Ctx, into_response::IntoResponse};

pub type Handler = Arc<dyn HandlerRun>;

pub(crate) enum HandlerType {
    Middleware,
    Method(Method),
}

impl std::fmt::Display for HandlerType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HandlerType::Middleware => write!(f, "Middleware"),
            HandlerType::Method(method) => write!(f, "Method({method})"),
        }
    }
}

pub(crate) struct HandlerWrapper<F> {
    pub(crate) f: F,
    pub(crate) handler_type: HandlerType,
    #[cfg(debug_assertions)]
    pub(crate) location: String,
}

impl<F> HandlerWrapper<F> {
    pub(crate) fn new(f: F, handler_type: HandlerType, _skip: usize) -> Self {
        #[cfg(debug_assertions)]
        {
            Self {
                f,
                handler_type,
                location: caller_location(_skip),
            }
        }
        #[cfg(not(debug_assertions))]
        {
            Self { f, handler_type }
        }
    }
}

impl<F> Debug for HandlerWrapper<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[cfg(debug_assertions)]
        {
            write!(f, "{}: {}", self.handler_type, self.location)
        }
        #[cfg(not(debug_assertions))]
        {
            write!(f, "HandlerWrapper")
        }
    }
}

#[async_trait]
pub(crate) trait HandlerRun: Send + Sync + Debug {
    async fn run(&self, ctx: &mut Ctx);
    fn handler_type(&self) -> &HandlerType;
}

#[async_trait]
impl<F, R> HandlerRun for HandlerWrapper<F>
where
    for<'a> F: AsyncFn1<&'a mut Ctx, Output = R> + Send + Sync,
    R: IntoResponse + Send,
{
    async fn run(&self, ctx: &mut Ctx) {
        let result = (self.f)(ctx).await;
        result.into_response(ctx);
    }

    fn handler_type(&self) -> &HandlerType {
        &self.handler_type
    }
}

#[cfg(debug_assertions)]
fn caller_location(skip: usize) -> String {
    let bt = backtrace::Backtrace::new();
    for frame in bt.frames().iter().skip(skip) {
        for symbol in frame.symbols() {
            if let (Some(file), Some(line)) = (symbol.filename(), symbol.lineno()) {
                return format!("{}:{}", file.display(), line);
            }
        }
    }
    String::new()
}
