use std::{any::Any, fmt::Debug, pin::Pin, sync::Arc};

use http::Method;

use crate::{
    async_fn::{AsyncFn1, AsyncFn2},
    ctx::Ctx,
    into_response::IntoResponse,
};

pub type Handler = Arc<dyn HandlerRun>;

pub enum HandlerType {
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

// Helper trait to unify handlers with/without state
pub trait HandlerCall<S>: Send + Sync {
    type Output: IntoResponse + Send;
    fn call(&self, c: &mut Ctx, state: &S) -> impl Future<Output = Self::Output> + Send;
}

impl<F, R> HandlerCall<()> for F
where
    for<'a> F: AsyncFn1<&'a mut Ctx, Output = R> + Send + Sync,
    R: IntoResponse + Send,
{
    type Output = R;
    async fn call(&self, c: &mut Ctx, _: &()) -> Self::Output {
        self.call(c).await
    }
}

impl<F, R, S> HandlerCall<(S,)> for F
where
    for<'a> F: AsyncFn2<&'a mut Ctx, S, Output = R> + Send + Sync,
    S: Clone + Sync,
    R: IntoResponse + Send,
{
    type Output = R;
    async fn call(&self, c: &mut Ctx, state: &(S,)) -> Self::Output {
        self.call(c, state.0.clone()).await
    }
}

pub(crate) struct HandlerWrapper<F, S = ()> {
    pub(crate) f: F,
    pub(crate) state: S,
    pub(crate) handler_type: HandlerType,
    #[cfg(debug_assertions)]
    pub(crate) location: String,
}

impl<F, S> HandlerWrapper<F, S> {
    pub(crate) fn new(f: F, state: S, handler_type: HandlerType, _skip: usize) -> Self {
        #[cfg(debug_assertions)]
        {
            Self {
                f,
                state,
                handler_type,
                location: caller_location(_skip),
            }
        }
        #[cfg(not(debug_assertions))]
        {
            Self {
                f,
                state,
                handler_type,
            }
        }
    }
}

impl<F, S> Debug for HandlerWrapper<F, S> {
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

pub trait HandlerRun: Send + Sync + Debug {
    fn run<'s, 'c, 'a>(&'s self, c: &'c mut Ctx) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>
    where
        's: 'a,
        'c: 'a,
        Self: 'a;

    fn handler_type(&self) -> &HandlerType;
    fn state(&self) -> &dyn Any;
}

impl dyn HandlerRun {
    pub fn get_state<S: 'static>(&self) -> Option<&S> {
        self.state().downcast_ref::<(S,)>().map(|s| &s.0)
    }
}

impl<F, S> HandlerRun for HandlerWrapper<F, S>
where
    F: HandlerCall<S>,
    S: Clone + Send + Sync + 'static,
{
    fn run<'s, 'c, 'a>(&'s self, c: &'c mut Ctx) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>
    where
        's: 'a,
        'c: 'a,
        Self: 'a,
    {
        Box::pin(async move {
            let result = self.f.call(c, &self.state).await;
            result.into_response(c);
        })
    }

    fn handler_type(&self) -> &HandlerType {
        &self.handler_type
    }

    fn state(&self) -> &dyn Any {
        &self.state
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
