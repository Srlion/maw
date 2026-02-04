use std::{
    any::Any,
    fmt::{self, Debug},
    future::Future,
    pin::Pin,
    sync::Arc,
};

use http::Method;

use crate::{app::App, ctx::Ctx, into_response::IntoResponse};

pub type DynHandlerRun = Arc<dyn HandlerRun>;

pub enum HandlerType {
    Middleware,
    Method(Method),
}

impl fmt::Display for HandlerType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HandlerType::Middleware => write!(f, "Middleware"),
            HandlerType::Method(method) => write!(f, "Method({method})"),
        }
    }
}

#[allow(non_snake_case)]
pub trait Handler<Ctx> {
    type Output;
    fn call(&self, c: Ctx) -> impl Future<Output = Self::Output> + Send;

    #[allow(unused_variables)]
    fn on_app_listen_mut(&self, app: &mut App) {}

    #[allow(unused_variables)]
    fn on_app_listen_arc(&self, app: &Arc<App>) {}

    fn state(&self) -> &dyn std::any::Any {
        &()
    }
}

impl<F: ?Sized, Fut, Ctx> Handler<Ctx> for F
where
    F: Fn(Ctx) -> Fut,
    Fut: Future + Send,
{
    type Output = Fut::Output;
    fn call(&self, c: Ctx) -> impl Future<Output = Self::Output> + Send {
        (self)(c)
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
        Self {
            f,
            handler_type,
            #[cfg(debug_assertions)]
            location: caller_location(_skip),
        }
    }
}

impl<F> Debug for HandlerWrapper<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.handler_type)?;
        #[cfg(debug_assertions)]
        write!(f, ": {}", self.location)?;
        Ok(())
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

    fn on_app_listen_mut(&self, _: &mut crate::app::App);
    fn on_app_listen_arc(&self, _: &Arc<crate::app::App>);

    fn type_id(&self) -> std::any::TypeId
    where
        Self: 'static;
}

impl dyn HandlerRun {
    pub fn get_state<S: 'static>(&self) -> Option<&S> {
        self.state().downcast_ref::<(S,)>().map(|s| &s.0)
    }
}

impl<F, R> HandlerRun for HandlerWrapper<F>
where
    F: for<'a> Handler<&'a mut Ctx, Output = R> + Send + Sync + 'static,
    R: IntoResponse + Send,
{
    fn run<'s, 'c, 'a>(&'s self, c: &'c mut Ctx) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>
    where
        's: 'a,
        'c: 'a,
        Self: 'a,
    {
        Box::pin(async move {
            self.f.call(c).await.into_response(c);
        })
    }

    fn handler_type(&self) -> &HandlerType {
        &self.handler_type
    }

    fn state(&self) -> &dyn Any {
        self.f.state()
    }

    fn on_app_listen_mut(&self, a: &mut App) {
        self.f.on_app_listen_mut(a);
    }

    fn on_app_listen_arc(&self, a: &Arc<App>) {
        self.f.on_app_listen_arc(a);
    }

    fn type_id(&self) -> std::any::TypeId {
        self.f.type_id()
    }
}

#[cfg(debug_assertions)]
fn caller_location(skip: usize) -> String {
    let bt = std::backtrace::Backtrace::force_capture();
    let s = format!("{bt:?}");

    s.split("{ fn: ")
        .filter_map(|chunk| {
            let file_start = chunk.find("file: \"")?;
            let file_end = chunk[file_start + 7..].find('"')?;
            let file = &chunk[file_start + 7..file_start + 7 + file_end];

            let line_start = chunk.find("line: ")?;
            let line_end = chunk[line_start + 6..].find(' ')?;
            let line = &chunk[line_start + 6..line_start + 6 + line_end];

            Some(format!("{file}:{line}"))
        })
        .nth(skip)
        .unwrap_or_default()
}
