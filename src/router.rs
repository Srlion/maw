use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use http::Method;

use crate::{
    async_fn::AsyncFn1,
    ctx::Ctx,
    handler::{Handler, HandlerType, HandlerWrapper},
    into_response::IntoResponse,
};

pub type Handlers = HashMap<Method, Arc<[Handler]>>;

pub(crate) type MatchRouter = matchit::Router<Handlers>;

#[derive(Clone)]
pub(crate) enum RouterItem {
    Handler(Handler),
    Child(Box<Router>),
}

// No actual need for interior mutability here, but just to make things easier for the user
// It's not like this will be a performance bottleneck
#[derive(Clone, Default)]
pub struct Router {
    path: String,
    items: Arc<Mutex<Vec<RouterItem>>>,
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
        for<'a> F: AsyncFn1<&'a mut Ctx, Output = R> + Send + Sync + 'static,
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
    fn middleware_impl<F, R>(&self, f: F, skip: usize) -> &Self
    where
        for<'a> F: AsyncFn1<&'a mut Ctx, Output = R> + Send + Sync + 'static,
        R: IntoResponse + Send,
    {
        let handler = Arc::new(HandlerWrapper::new(f, HandlerType::Middleware {}, skip));
        self.items
            .lock()
            .unwrap()
            .push(RouterItem::Handler(handler));
        self
    }

    #[inline(never)]
    pub fn middleware<F, R>(&self, f: F) -> &Self
    where
        for<'a> F: AsyncFn1<&'a mut Ctx, Output = R> + Send + Sync + 'static,
        R: IntoResponse + Send,
    {
        self.middleware_impl(f, 4)
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
    fn flatten_routers(&self) -> HashMap<String, Handlers> {
        let mut out = HashMap::default();
        Self::walk("", self, &[], &mut out);
        out
    }

    #[inline(never)]
    fn walk(
        base: &str,
        router: &Router,
        inherited_mw: &[Handler],
        out: &mut HashMap<String, Handlers>,
    ) {
        let path = join_paths(base, &router.path);

        let mut method_handlers: HashMap<Method, Arc<[Handler]>> = HashMap::default();
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

impl<F, R> AddHandlers for F
where
    for<'a> F: AsyncFn1<&'a mut Ctx, Output = R> + Send + Sync + 'static,
    R: IntoResponse + Send,
{
    fn add_handlers(
        self,
        router: &Router,
        method: Method,
        path: impl Into<String>,
        skip: usize,
    ) -> Router {
        let group = Router::group(path);
        let group = group.handle(method, self, skip);
        router.push(group)
    }
}

macro_rules! impl_add_handlers {
    ($(($($f:ident, $r:ident),+; $last_f:ident, $last_r:ident)),+ $(,)?) => {
        $(
            impl<$($f, $r,)+ $last_f, $last_r> AddHandlers for ($($f,)+ $last_f,)
            where
                $(
                    for<'a> $f: AsyncFn1<&'a mut Ctx, Output = $r> + Send + Sync + 'static,
                    $r: IntoResponse + Send,
                )+
                for<'a> $last_f: AsyncFn1<&'a mut Ctx, Output = $last_r> + Send + Sync + 'static,
                $last_r: IntoResponse + Send,
            {
                fn add_handlers(
                    self,
                    router: &Router,
                    method: Method,
                    path: impl Into<String>,
                    skip: usize,
                ) -> Router {
                    #[allow(non_snake_case)]
                    let ($($f,)+ $last_f,) = self;
                    router.push(
                        Router::group(path)
                            $(.middleware_impl($f, skip))+
                            .handle(method, $last_f, skip)
                    )
                }
            }
        )+
    };
}

impl_add_handlers! {
    (F1, R1; F2, R2),
    (F1, R1, F2, R2; F3, R3),
    (F1, R1, F2, R2, F3, R3; F4, R4),
    (F1, R1, F2, R2, F3, R3, F4, R4; F5, R5),
    (F1, R1, F2, R2, F3, R3, F4, R4, F5, R5; F6, R6),
    (F1, R1, F2, R2, F3, R3, F4, R4, F5, R5, F6, R6; F7, R7),
    (F1, R1, F2, R2, F3, R3, F4, R4, F5, R5, F6, R6, F7, R7; F8, R8),
    (F1, R1, F2, R2, F3, R3, F4, R4, F5, R5, F6, R6, F7, R7, F8, R8; F9, R9),
    (F1, R1, F2, R2, F3, R3, F4, R4, F5, R5, F6, R6, F7, R7, F8, R8, F9, R9; F10, R10),
}
