use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex},
};

use http::Method;

use crate::{
    ctx::Ctx,
    handler::Handler,
    handler::{DynHandlerRun, HandlerType, HandlerWrapper},
    into_response::IntoResponse,
};

pub type Handlers = HashMap<Method, Arc<[DynHandlerRun]>>;

pub(crate) type MatchRouter = matchit::Router<Handlers>;

#[derive(Clone)]
pub(crate) enum RouterItem {
    Handler(DynHandlerRun),
    Child(Box<Router>),
}

// No actual need for interior mutability here, but just to make things easier for the user
// It's not like this will be a performance bottleneck
#[derive(Clone, Default)]
pub struct Router {
    path: String,
    items: Arc<Mutex<Vec<RouterItem>>>,
}

pub struct WithState<S, F>(pub S, pub F);

impl<'a, S, F, Fut> Handler<&'a mut Ctx> for WithState<S, F>
where
    S: Clone + Send + Sync + 'static,
    F: Fn(&'a mut Ctx, S) -> Fut + Sync,
    Fut: Future + Send,
{
    type Output = Fut::Output;

    async fn call(&self, c: &'a mut Ctx) -> Self::Output {
        (self.1)(c, self.0.clone()).await
    }

    fn state(&self) -> &dyn std::any::Any {
        &self.0
    }
}

impl Router {
    #[inline(never)]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline(never)]
    pub fn push(&self, router: Router) -> Self {
        self.items
            .lock()
            .unwrap()
            .push(RouterItem::Child(Box::new(router)));
        self.clone()
    }

    #[inline(never)]
    pub fn group(path: impl Into<String>) -> Self {
        let path = path.into();
        if path != "/" && (!path.starts_with('/') || path.ends_with('/')) {
            panic!("Path must start with / and not end with / - got {path}");
        }
        Self {
            path,
            items: Arc::default(),
        }
    }

    #[inline(never)]
    fn handle<F, R>(&self, method: Method, f: F, skip: usize) -> Self
    where
        F: for<'a> Handler<&'a mut Ctx, Output = R> + Send + Sync + 'static,
        R: IntoResponse + Send,
    {
        let handler = Arc::new(HandlerWrapper::new(f, HandlerType::Method(method), skip));
        self.items
            .lock()
            .unwrap()
            .push(RouterItem::Handler(handler));
        self.clone()
    }

    #[inline(never)]
    pub fn add(&self, method: Method, path: impl Into<String>, handlers: impl AddHandlers) -> Self {
        handlers.add_handlers(self, method, path, 5)
    }

    #[inline(never)]
    fn middleware_impl<F, R>(&self, f: F, skip: usize) -> Self
    where
        F: for<'a> Handler<&'a mut Ctx, Output = R> + Send + Sync + 'static,
        R: IntoResponse + Send,
    {
        let handler = Arc::new(HandlerWrapper::new(f, HandlerType::Middleware, skip));
        self.items
            .lock()
            .unwrap()
            .push(RouterItem::Handler(handler));
        self.clone()
    }

    #[inline(never)]
    pub fn middleware(&self, handlers: impl AddMiddleware) -> Self {
        handlers.add_middleware(self, 5)
    }

    #[inline(never)]
    pub(crate) fn build(&self) -> Result<MatchRouter, matchit::InsertError> {
        let mut match_router = matchit::Router::new();

        for (path, handlers) in self.flatten_routers() {
            match_router.insert(path, handlers)?;
        }

        Ok(match_router)
    }

    #[inline(never)]
    pub(crate) fn flatten_routers(&self) -> BTreeMap<String, Handlers> {
        let mut out = BTreeMap::default();
        Self::walk("", self, &[], &mut out);
        out
    }

    #[inline(never)]
    fn walk(
        base: &str,
        router: &Router,
        inherited_mw: &[DynHandlerRun],
        out: &mut BTreeMap<String, Handlers>,
    ) {
        let path = join_paths(base, &router.path);

        let mut method_handlers: HashMap<Method, Arc<[DynHandlerRun]>> = HashMap::default();
        let mut inherited_for_children = inherited_mw.to_vec(); // Only global middlewares for children

        // Process items in order
        for item in router.items.lock().unwrap().iter() {
            match item {
                RouterItem::Handler(h) => match h.handler_type() {
                    HandlerType::Middleware => {
                        inherited_for_children.push(h.clone());
                    }
                    HandlerType::Method(method) => {
                        // Build the complete chain for this method with all middlewares seen so far
                        let mut chain = inherited_for_children.clone();
                        chain.push(h.clone());

                        // Check for conflicts within this router
                        if let Some(existing) = method_handlers.get(method) {
                            panic!(
                                "Handler for method {} already exists at path {}\nExisting: {:?}\nNew: {:?}",
                                method, path, existing, h
                            );
                        }
                        method_handlers.insert(method.clone(), Arc::from(chain.into_boxed_slice()));
                    }
                },
                RouterItem::Child(child) => {
                    Self::walk(&path, child, &inherited_for_children, out);
                }
            }
        }

        // Add to output if we have handlers
        if !method_handlers.is_empty() {
            // Check for conflicts when merging with existing handlers
            let entry = out.entry(path.clone()).or_default();
            for (method, new_handler_chain) in &method_handlers {
                if let Some(existing_handler_chain) = entry.get(method) {
                    panic!(
                        "Handler for method {} already exists at path {}\nExisting: {:?}\nNew: {:?}",
                        method,
                        path,
                        existing_handler_chain.last(),
                        new_handler_chain.last()
                    );
                }
            }
            entry.extend(method_handlers);
        }
    }

    #[inline(never)]
    pub fn all(&self, path: impl Into<String>, handlers: impl AddHandlers) -> Self {
        handlers.add_handlers(self, crate::all(), path, 5)
    }

    method_handlers!(GET, POST, PUT, DELETE, HEAD, OPTIONS, CONNECT, PATCH, TRACE);

    #[cfg(feature = "static_files")]
    #[inline(never)]
    pub fn static_files<E: rust_embed::RustEmbed + Send + Sync + 'static>(
        &self,
        prefix: &'static str,
        files: crate::static_files::StaticFiles<E>,
    ) -> Self {
        let prefix = prefix.trim_matches('/');
        let has_index = E::get(files.index).is_some();

        let (root, catch_all) = if prefix.is_empty() {
            ("/".to_string(), "/{*_}".to_string())
        } else {
            (["/", prefix].concat(), ["/", prefix, "/{*_}"].concat())
        };

        let r = if has_index {
            files.clone().add_handlers(self, Method::GET, root, 5)
        } else {
            self.clone()
        };
        files.add_handlers(&r, Method::GET, catch_all, 5)
    }
}

impl std::fmt::Debug for Router {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{:#?}", self.flatten_routers())?;
        Ok(())
    }
}

#[inline(never)]
fn join_paths(parent: &str, child: &str) -> String {
    match (parent, child) {
        ("", "") => "/".to_string(),
        ("", c) => c.to_string(),
        (p, "") => p.to_string(),
        (p, "/") => p.to_string(),
        ("/", c) => c.to_string(),
        (p, c) => format!("{p}{c}"),
    }
}

macro_rules! method_handlers {
    ($($method:ident),* $(,)?) => {
        $(
            paste::paste! {
                #[inline(never)]
                pub fn [<$method:lower>](&self, path: impl Into<String>, handlers: impl AddHandlers) -> Self
                {
                    handlers.add_handlers(self, Method::$method, path, 5)
                }
            }
        )*
    };
}
use method_handlers;

pub trait AddHandlers {
    fn add_handlers(
        self,
        router: &Router,
        method: Method,
        path: impl Into<String>,
        skip: usize,
    ) -> Router;
}

pub trait AddMiddleware {
    fn add_middleware(self, router: &Router, skip: usize) -> Router;
}

pub trait IntoHandler {
    fn into_middleware(self, router: &Router, skip: usize) -> Router;
    fn into_handler(self, router: &Router, method: Method, skip: usize) -> Router;
}

impl<F, R> IntoHandler for F
where
    F: for<'a> Handler<&'a mut Ctx, Output = R> + Send + Sync + 'static,
    R: IntoResponse + Send,
{
    fn into_middleware(self, router: &Router, skip: usize) -> Router {
        router.middleware_impl(self, skip + 1)
    }

    fn into_handler(self, router: &Router, method: Method, skip: usize) -> Router {
        router.handle(method, self, skip + 1)
    }
}

// F1 != (F1,)
impl<F1: IntoHandler> AddMiddleware for F1 {
    #[allow(non_snake_case)]
    fn add_middleware(self, r: &Router, skip: usize) -> Router {
        self.into_middleware(r, skip)
    }
}

impl<F1> AddHandlers for F1
where
    F1: IntoHandler,
{
    fn add_handlers(
        self,
        router: &Router,
        method: Method,
        path: impl Into<String>,
        skip: usize,
    ) -> Router {
        let group = Router::group(path);
        router.push(self.into_handler(&group, method, skip))
    }
}

macro_rules! impl_add_handlers {
    ($(($($prev:ident),*; $last:ident)),+ $(,)?) => {
        $(
            impl<$($prev: IntoHandler,)* $last: IntoHandler> AddHandlers for ($($prev,)* $last,) {
                fn add_handlers(
                    self,
                    router: &Router,
                    method: Method,
                    path: impl Into<String>,
                    skip: usize,
                ) -> Router {
                    #[allow(non_snake_case)]
                    let ($($prev,)* $last,) = self;
                    let group = Router::group(path);
                    $(
                        $prev.into_middleware(&group, skip);
                    )*
                    router.push(
                        $last.into_handler(&group, method, skip)
                    )
                }
            }

            impl<$($prev: IntoHandler,)* $last: IntoHandler> AddMiddleware for ($($prev,)* $last,) {
                #[allow(non_snake_case)]
                fn add_middleware(self, r: &Router, skip: usize) -> Router {
                    let ($($prev,)* $last,) = self;
                    $( $prev.into_middleware(r, skip); )*
                    $last.into_middleware(r, skip)
                }
            }
        )+
    };
}
impl_add_handlers!(
    (; F1),
    (F1; F2),
    (F1, F2; F3),
    (F1, F2, F3; F4),
    (F1, F2, F3, F4; F5),
    (F1, F2, F3, F4, F5; F6),
    (F1, F2, F3, F4, F5, F6; F7),
    (F1, F2, F3, F4, F5, F6, F7; F8),
    (F1, F2, F3, F4, F5, F6, F7, F8; F9),
    (F1, F2, F3, F4, F5, F6, F7, F8, F9; F10),
);
