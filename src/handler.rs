use std::{
    fmt::Debug,
    sync::{Arc, atomic::AtomicBool},
};

use async_trait::async_trait;
use http::Method;

use crate::{async_fn::AsyncFn1, ctx::Ctx, status_error::StatusError};

pub(crate) type HandlerOutput = Result<(), StatusError>;

pub type Handler = Arc<dyn HandlerRun>;

pub(crate) enum HandlerType {
    Middleware {
        is_global: bool,
    },
    Method {
        method: Method,
        use_as_head: AtomicBool,
    },
}

impl std::fmt::Display for HandlerType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HandlerType::Middleware { is_global } => {
                // if global, print Middleware(Global), else Middleware(Local)
                if *is_global {
                    write!(f, "Middleware(Global)")
                } else {
                    write!(f, "Middleware(Local)")
                }
            }
            HandlerType::Method { method, .. } => write!(f, "Method({})", method),
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
    pub(crate) fn new(f: F, handler_type: HandlerType, skip: usize) -> Self {
        #[cfg(debug_assertions)]
        {
            Self {
                f,
                handler_type,
                location: caller_location(skip),
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
    async fn run(&self, ctx: &mut Ctx) -> HandlerOutput;
    fn handler_type(&self) -> &HandlerType;
    fn set_use_as_head(&self, value: bool);
}

#[async_trait]
impl<F> HandlerRun for HandlerWrapper<F>
where
    for<'a> F: AsyncFn1<&'a mut Ctx, Output = HandlerOutput> + Send + Sync,
{
    async fn run(&self, ctx: &mut Ctx) -> HandlerOutput {
        (self.f)(ctx).await
    }

    fn handler_type(&self) -> &HandlerType {
        &self.handler_type
    }

    fn set_use_as_head(&self, value: bool) {
        match &self.handler_type {
            HandlerType::Method {
                method,
                use_as_head,
            } => {
                if *method == Method::GET {
                    use_as_head.store(value, std::sync::atomic::Ordering::SeqCst);
                } else {
                    panic!("can only set use_as_head on GET method handlers");
                }
            }
            HandlerType::Middleware { .. } => {
                panic!("cannot set use_as_head on a middleware handler")
            }
        }
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
