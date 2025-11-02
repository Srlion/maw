use std::sync::Arc;

use http::Method;
use rustc_hash::FxHashMap;

use crate::{
    async_fn::{AsyncFn2, AsyncFn3},
    handler::{HandlerOutput, HandlerWrapper, MethodHandler, MiddlewareHandler},
    next::Next,
    request::Request,
    response::Response,
};

pub(crate) type MatchRouter = Arc<matchit::Router<Arc<Handlers>>>;

#[derive(Clone, Default, Debug)]
pub struct Handlers {
    pub(crate) middlewares: Vec<MiddlewareHandler>,
    pub(crate) methods: FxHashMap<Method, MethodHandler>,
    pub(crate) all: Option<MethodHandler>,
}

#[derive(Clone, Default)]
pub struct Router {
    path: String,
    handler: FxHashMap<Method, MethodHandler>,
    all_handler: Option<MethodHandler>,
    children: Vec<Router>,
    global_middlewares: Vec<MiddlewareHandler>, // global middlewares
    local_middlewares: Vec<MiddlewareHandler>,  // local middlewares
}

impl Router {
    #[inline(never)]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline(never)]
    pub fn push(mut self, router: Router) -> Self {
        self.children.push(router);
        self
    }

    #[inline(never)]
    pub fn with_path(path: impl Into<String>) -> Self {
        let path = path.into();
        if path != "/" && (!path.starts_with('/') || path.ends_with('/')) {
            panic!("Path must start with / and not end with / - got {path}");
        }
        Self {
            path,
            ..Self::default()
        }
    }

    #[inline(never)]
    pub fn handle<F>(mut self, method: Method, f: F) -> Self
    where
        for<'a> F: AsyncFn2<&'a mut Request, &'a mut Response, Output = HandlerOutput>
            + Send
            + Sync
            + 'static,
    {
        assert!(
            !self.handler.contains_key(&method),
            "Handler for method {method} already exists on path {}",
            self.path
        );
        let f = Arc::new(HandlerWrapper::new(f, ""));
        self.handler.insert(method.clone(), f);
        self
    }

    #[inline(never)]
    pub fn route<F>(self, method: Method, path: impl Into<String>, f: F) -> Self
    where
        for<'a> F: AsyncFn2<&'a mut Request, &'a mut Response, Output = HandlerOutput>
            + Send
            + Sync
            + 'static,
    {
        self.push(Router::with_path(path).handle(method, f))
    }

    /// Global middleware (inherited by children)
    #[inline(never)]
    pub fn middleware<F>(mut self, f: F) -> Self
    where
        for<'a> F: AsyncFn3<&'a mut Request, &'a mut Response, Next, Output = HandlerOutput>
            + Send
            + Sync
            + 'static,
    {
        self.global_middlewares
            .push(Arc::new(HandlerWrapper::new(f, "Global")));
        self
    }

    /// Local middleware (only for this route, not inherited by children)
    #[inline(never)]
    pub fn local_middleware<F>(mut self, f: F) -> Self
    where
        for<'a> F: AsyncFn3<&'a mut Request, &'a mut Response, Next, Output = HandlerOutput>
            + Send
            + Sync
            + 'static,
    {
        self.local_middlewares
            .push(Arc::new(HandlerWrapper::new(f, "Local")));
        self
    }

    #[inline(never)]
    pub fn all<F>(mut self, f: F) -> Self
    where
        for<'a> F: AsyncFn2<&'a mut Request, &'a mut Response, Output = HandlerOutput>
            + Send
            + Sync
            + 'static,
    {
        assert!(
            self.all_handler.is_none(),
            "All handler already exists on path {}",
            self.path
        );
        let f = Arc::new(HandlerWrapper::new(f, ""));
        self.all_handler = Some(f);
        self
    }

    #[inline(never)]
    pub(crate) fn build(&self) -> Result<MatchRouter, matchit::InsertError> {
        let mut match_router = matchit::Router::new();

        for (path, handlers) in self.flatten_routers() {
            match_router.insert(path, Arc::from(handlers))?;
        }

        Ok(Arc::new(match_router))
    }

    #[inline(never)]
    fn flatten_routers(&self) -> Vec<(String, Handlers)> {
        let mut out = Vec::new();
        Self::walk("", self, &Handlers::default(), &mut out);
        out
    }

    #[inline(never)]
    fn walk(
        base: &str,
        router: &Router,
        inherited_mw: &Handlers,
        out: &mut Vec<(String, Handlers)>,
    ) {
        let path = join_paths(base, &router.path);

        let mut next_inherited = inherited_mw.clone();
        next_inherited
            .middlewares
            .extend(router.global_middlewares.iter().cloned());

        if router.handler.is_empty() && !router.local_middlewares.is_empty() {
            tracing::warn!("Route {path} has local middlewares but no handlers!");
        }

        if !router.handler.is_empty() || router.all_handler.is_some() {
            // Order: global (inherited + this), local then handlers
            let mut handlers = next_inherited.clone();
            handlers
                .middlewares
                .extend(router.local_middlewares.iter().cloned());

            handlers.methods = router.handler.clone();
            handlers.all = router.all_handler.clone();

            out.push((path.clone(), handlers));
        }

        for child in &router.children {
            Self::walk(&path, child, &next_inherited, out);
        }
    }

    method_handlers!(GET, POST, PUT, DELETE, HEAD, OPTIONS, CONNECT, PATCH, TRACE);
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
        ("/", c) => c.to_string(),
        (p, c) => format!("{p}{c}"),
    }
}

macro_rules! method_handlers {
    ($($method:ident),* $(,)?) => {
        $(
            paste::paste! {
                #[inline(never)]
                pub fn [<$method:lower>]<F>(self, f: F) -> Self
                where
                    for<'a> F: AsyncFn2<&'a mut Request, &'a mut Response, Output = HandlerOutput>
                        + Send
                        + Sync
                        + 'static,
                {
                    self.handle(Method::$method, f)
                }
            }
        )*
    };
}

use method_handlers;
