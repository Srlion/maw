use std::sync::{Arc, atomic::AtomicBool};

use http::Method;

use crate::{
    async_fn::AsyncFn1,
    ctx::Ctx,
    handler::{Handler, HandlerType, HandlerWrapper},
    into_response::IntoResponse,
};

pub(crate) type MatchRouter = Arc<matchit::Router<Arc<[Handler]>>>;

#[derive(Clone)]
pub(crate) enum RouterItem {
    Handler(Handler),
    Child(Box<Router>),
}

#[derive(Clone, Default)]
pub struct Router {
    path: String,
    items: Vec<RouterItem>,
}

impl Router {
    #[inline(never)]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline(never)]
    pub fn push(mut self, router: Router) -> Self {
        self.items.push(RouterItem::Child(Box::new(router)));
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
            items: Vec::new(),
        }
    }

    #[inline(never)]
    fn handle_impl<F, R>(mut self, method: Method, f: F, skip: usize) -> Self
    where
        for<'a> F: AsyncFn1<&'a mut Ctx, Output = R> + Send + Sync + 'static,
        R: IntoResponse + Send,
    {
        for item in &self.items {
            if let RouterItem::Handler(existing) = item
                && let HandlerType::Method {
                    method: existing_method,
                    ..
                } = existing.handler_type()
                && *existing_method == method
            {
                panic!("Handler for method {} already exists in this route", method);
            }
        }
        let handler = Arc::new(HandlerWrapper::new(
            f,
            HandlerType::Method {
                method,
                use_as_head: AtomicBool::new(false),
            },
            skip,
        ));
        self.items.push(RouterItem::Handler(handler));
        self
    }

    #[inline(never)]
    pub fn handle<F, R>(self, method: Method, f: F) -> Self
    where
        for<'a> F: AsyncFn1<&'a mut Ctx, Output = R> + Send + Sync + 'static,
        R: IntoResponse + Send,
    {
        self.handle_impl(method, f, 4)
    }

    #[inline(never)]
    pub fn route<F, R>(self, method: Method, path: impl Into<String>, f: F) -> Self
    where
        for<'a> F: AsyncFn1<&'a mut Ctx, Output = R> + Send + Sync + 'static,
        R: IntoResponse + Send,
    {
        self.push(Router::with_path(path).handle_impl(method, f, 4))
    }

    /// Global middleware (inherited by children)
    #[inline(never)]
    pub fn middleware<F, R>(mut self, f: F) -> Self
    where
        for<'a> F: AsyncFn1<&'a mut Ctx, Output = R> + Send + Sync + 'static,
        R: IntoResponse + Send,
    {
        let handler = Arc::new(HandlerWrapper::new(
            f,
            HandlerType::Middleware { is_global: true },
            3,
        ));
        self.items.push(RouterItem::Handler(handler));
        self
    }

    /// Local middleware (only for this route, not inherited by children)
    #[inline(never)]
    pub fn local_middleware<F, R>(mut self, f: F) -> Self
    where
        for<'a> F: AsyncFn1<&'a mut Ctx, Output = R> + Send + Sync + 'static,
        R: IntoResponse + Send,
    {
        let handler = Arc::new(HandlerWrapper::new(
            f,
            HandlerType::Middleware { is_global: false },
            3,
        ));
        self.items.push(RouterItem::Handler(handler));
        self
    }

    #[inline(never)]
    pub(crate) fn build(&self) -> Result<MatchRouter, matchit::InsertError> {
        let mut match_router = matchit::Router::new();

        for (path, handlers) in self.flatten_routers() {
            mark_get_as_head(&handlers);
            match_router.insert(path, Arc::from(handlers))?;
        }

        Ok(Arc::new(match_router))
    }

    #[inline(never)]
    fn flatten_routers(&self) -> Vec<(String, Vec<Handler>)> {
        let mut out = Vec::new();
        Self::walk("", self, &[], &mut out);
        out
    }

    #[inline(never)]
    fn walk(
        base: &str,
        router: &Router,
        inherited_mw: &[Handler],
        out: &mut Vec<(String, Vec<Handler>)>,
    ) {
        let path = join_paths(base, &router.path);

        let mut local_handlers = Vec::new();
        let mut new_global_mw = Vec::new();
        let mut next_inherited = inherited_mw.to_vec();

        // Process items in order
        for item in &router.items {
            match item {
                RouterItem::Handler(h) => match h.handler_type() {
                    HandlerType::Middleware { is_global } => {
                        if *is_global {
                            new_global_mw.push(h.clone());
                            next_inherited.push(h.clone());
                        } else {
                            local_handlers.push(h.clone());
                        }
                    }
                    HandlerType::Method { .. } => {
                        local_handlers.push(h.clone());
                    }
                },
                RouterItem::Child(child) => {
                    // Process child with inherited middlewares
                    Self::walk(&path, child, &next_inherited, out);
                }
            }
        }

        // Only push if we have local handlers or new global middleware
        if !local_handlers.is_empty() || !new_global_mw.is_empty() {
            let mut final_handlers = inherited_mw.to_vec();
            final_handlers.extend(new_global_mw);
            final_handlers.extend(local_handlers);
            out.push((path, final_handlers));
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

/// This is a simple way, to allow using GET handlers for HEAD requests
/// by marking all GET handlers to also be used for HEAD.
/// Their body will be ignored later in the request handling.
#[inline(never)]
fn mark_get_as_head(handlers: &[Handler]) {
    let mut get_handler: Option<&Handler> = None;
    for handler in handlers {
        if let HandlerType::Method { method, .. } = handler.handler_type() {
            match *method {
                Method::HEAD => return, // Early exit if HEAD exists
                Method::GET => get_handler = Some(handler),
                _ => {}
            }
        }
    }
    if let Some(get) = get_handler {
        get.set_use_as_head(true);
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
                pub fn [<$method:lower>]<F, R>(self, f: F) -> Self
                where
                    for<'a> F: AsyncFn1<&'a mut Ctx, Output = R>
                        + Send
                        + Sync
                        + 'static,
                    R: IntoResponse + Send,
                {
                    self.handle_impl(Method::$method, f, 4)
                }
            }
        )*
    };
}

use method_handlers;
