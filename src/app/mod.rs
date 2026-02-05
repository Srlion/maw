use std::{
    collections::HashSet,
    net,
    sync::{Arc, RwLock},
};

use http::StatusCode;
use hyper::{Request as HyperRequest, body::Incoming as IncomingBody};
use hyper_util::rt::{TokioExecutor, TokioIo};
use smol_str::SmolStr;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

pub(crate) mod config;

#[cfg(feature = "minijinja")]
mod jinja;
#[cfg(feature = "minijinja")]
pub use jinja::Jinja;

use crate::{
    ALL,
    any_map::{AnyMap, SerializableAny},
    error::Error,
    request::Request,
    response::{HttpBody, Response},
    router::{self, MatchRouter},
};

type HttpResponse = http::Response<HttpBody>;

pub struct App<S = ()> {
    pub state: Arc<S>,
    pub(crate) router: router::Router,
    #[cfg(feature = "minijinja")]
    pub jinja: Jinja,
    pub(crate) locals: RwLock<AnyMap<dyn SerializableAny>>,
    pub(crate) built_router: MatchRouter,
    pub(crate) config: config::Config,
    pub(crate) shutdown_timeout: std::time::Duration,
    dump_routes: bool,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        App {
            state: Arc::new(()),
            router: router::Router::new(),
            #[cfg(feature = "minijinja")]
            jinja: Jinja::default(),
            locals: RwLock::new(AnyMap::new()),
            built_router: MatchRouter::default(),
            config: config::Config::default(),
            shutdown_timeout: std::time::Duration::from_secs(10),
            dump_routes: false,
        }
    }

    pub fn with_state<S>(self, state: S) -> App<S> {
        App {
            state: Arc::new(state),
            router: self.router,
            #[cfg(feature = "minijinja")]
            jinja: self.jinja,
            locals: self.locals,
            built_router: self.built_router,
            config: self.config,
            shutdown_timeout: self.shutdown_timeout,
            dump_routes: self.dump_routes,
        }
    }

    pub fn config(mut self, config: config::Config) -> Self {
        self.config = config;
        self
    }

    /// Sets the router for the application.
    ///
    /// Changes to the router after the server has started will not take effect.
    pub fn router(mut self, router: router::Router) -> Self {
        self.router = router;
        self
    }

    /// Logs the complete route table at startup for debugging.
    ///
    /// When enabled, prints all registered routes with their HTTP methods,
    /// paths, and handler chains. Useful for verifying route configuration
    /// and debugging routing issues during development.
    pub fn dump_routes(mut self, enable: bool) -> Self {
        self.dump_routes = enable;
        self
    }

    /// Sets the graceful shutdown timeout duration.
    ///
    /// This is how long the server will wait for existing connections to close
    /// before forcing shutdown. Default is 10 seconds.
    pub fn shutdown_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.shutdown_timeout = timeout;
        self
    }

    /// Provides access to the application locals.
    pub fn locals<F>(&self, f: F) -> &Self
    where
        F: FnOnce(&AnyMap<dyn SerializableAny>),
    {
        let locals = self.locals.read().unwrap();
        f(&locals);
        self
    }

    /// Provides mutable access to the application locals.
    pub fn locals_mut<F>(&self, f: F) -> &Self
    where
        F: FnOnce(&mut AnyMap<dyn SerializableAny>),
    {
        let mut locals = self.locals.write().unwrap();
        f(&mut locals);
        self
    }

    /// Sets application locals.
    pub fn with_locals(self, f: impl FnOnce(&mut AnyMap<dyn SerializableAny>)) -> Self {
        self.locals_mut(f);
        self
    }

    /// set views path
    #[cfg(feature = "minijinja")]
    pub fn views(mut self, path: impl AsRef<std::path::Path>) -> Self {
        self.jinja = Jinja::new(path);
        self
    }

    #[cfg(feature = "minijinja")]
    pub fn views_with(
        mut self,
        path: impl AsRef<std::path::Path>,
        f: impl FnOnce(&mut minijinja::Environment<'static>),
    ) -> Self {
        self.jinja = Jinja::new(path);
        self.jinja.with(f);
        self
    }
}

impl App {
    /// Listen with ctrl+c shutdown
    pub async fn listen<A>(self, addr: A) -> Result<(), Error>
    where
        A: net::ToSocketAddrs + std::fmt::Debug + 'static,
    {
        let token = CancellationToken::new();
        let t = token.clone();
        tokio::spawn(async move {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to install CTRL+C signal handler");
            t.cancel();
        });
        self.listen_shutdown(addr, token).await
    }

    /// Listen with custom shutdown signal
    pub async fn listen_shutdown<A>(
        mut self,
        addr: A,
        shutdown: CancellationToken,
    ) -> Result<(), Error>
    where
        A: net::ToSocketAddrs + std::fmt::Debug + 'static,
    {
        if self.dump_routes {
            tracing::info!("App Router: {:#?}", self.router);
        }

        self.built_router = self.router.build()?;

        let middlewares: Vec<_> = {
            let mut called = HashSet::new();
            self.router
                .flatten_routers()
                .iter()
                .flat_map(|(_, m)| m.values())
                .flat_map(|h| h.iter())
                .filter(|h| called.insert(h.type_id()))
                .cloned()
                .collect()
        };

        for h in &middlewares {
            h.on_app_listen_mut(&mut self);
        }

        let arc_app = Arc::new(self);
        for h in &middlewares {
            h.on_app_listen_arc(&arc_app);
        }

        let addr = addr
            .to_socket_addrs()?
            .next()
            .ok_or(Error::FailedToParseAddr)?;

        let listener = TcpListener::bind(addr).await?;
        tracing::info!("Http app listening on http://{}", addr);

        let server = hyper_util::server::conn::auto::Builder::new(TokioExecutor::new());
        let graceful = hyper_util::server::graceful::GracefulShutdown::new();

        let _ = shutdown
            .run_until_cancelled(async {
                loop {
                    let Ok((stream, peer_addr)) = listener.accept().await else {
                        continue;
                    };
                    let io = TokioIo::new(stream);
                    let app = arc_app.clone();
                    let service = hyper::service::service_fn(move |req| {
                        handle_request(req, app.clone(), peer_addr)
                    });

                    let conn = server.serve_connection_with_upgrades(io, service);
                    let fut = graceful.watch(conn.into_owned());
                    tokio::spawn(async move {
                        if let Err(e) = fut.await {
                            tracing::trace!("connection failed: {e:?}");
                        }
                    });
                }
            })
            .await;

        tracing::info!("Shutdown signal received!");

        tracing::info!(
            "Waiting for connections to close (timeout: {:?})...",
            arc_app.shutdown_timeout
        );

        match tokio::time::timeout(arc_app.shutdown_timeout, graceful.shutdown()).await {
            Ok(_) => tracing::info!("All connections closed!"),
            Err(_) => tracing::info!("Shutdown timed out!"),
        }

        Ok(())
    }
}

impl Clone for App {
    fn clone(&self) -> Self {
        App {
            state: self.state.clone(),
            router: self.router.clone(),
            #[cfg(feature = "minijinja")]
            jinja: self.jinja.clone(),
            locals: RwLock::new(self.locals.read().unwrap().clone()),
            built_router: MatchRouter::default(),
            config: self.config.clone(),
            shutdown_timeout: self.shutdown_timeout,
            dump_routes: self.dump_routes,
        }
    }
}

async fn handle_request(
    request: HyperRequest<IncomingBody>,
    app: Arc<App>,
    peer_addr: net::SocketAddr,
) -> Result<HttpResponse, NoResponse> {
    let mut response = HttpResponse::new(HttpBody::default());

    let path = normalize_path(request.uri().path());
    let matched_route = match app.built_router.at(&path) {
        Ok(matched_route) => matched_route,
        Err(_) => {
            tracing::debug!("requested path not found: {path}");
            *response.status_mut() = StatusCode::NOT_FOUND;
            return Ok(response);
        }
    };

    let handlers = matched_route.value;
    let handlers = handlers
        .get(request.method())
        .or_else(|| {
            (request.method() == http::Method::HEAD)
                .then(|| handlers.get(&http::Method::GET))
                .flatten()
        })
        .or_else(|| handlers.get(&ALL));
    let Some(handlers) = handlers else {
        tracing::debug!(
            "requested method not allowed: {} {}",
            request.method(),
            path
        );
        *response.status_mut() = StatusCode::METHOD_NOT_ALLOWED;
        return Ok(response);
    };
    let handlers = handlers.clone();

    let params = matched_route
        .params
        .iter()
        .map(|(k, v)| (SmolStr::new(k), SmolStr::new(v)))
        .collect();

    let req = Request::new(app.clone(), request, params, peer_addr);
    let res = Response::from_response(app, response);

    let mut c = crate::ctx::Ctx::new(req, res, handlers);
    c.next().await;

    if c.is_closed() {
        return Err(NoResponse);
    }

    if c.req.method() == http::Method::HEAD {
        *c.res.inner.body_mut() = HttpBody::default();
    }

    Ok(c.res.inner)
}

fn normalize_path(s: &str) -> std::borrow::Cow<'_, str> {
    let mut result = None;

    for (i, ch) in s.char_indices() {
        if ch == '\\' {
            let mut owned = result.take().unwrap_or_else(|| {
                let mut buf = String::with_capacity(s.len());
                buf.push_str(&s[..i]);
                buf
            });
            owned.push('/');
            result = Some(owned);
        } else if let Some(ref mut owned) = result {
            owned.push(ch);
        }
    }

    match result {
        None => {
            if s.len() > 1 && s.ends_with('/') {
                std::borrow::Cow::Borrowed(&s[..s.len() - 1])
            } else {
                std::borrow::Cow::Borrowed(s)
            }
        }

        Some(mut owned) => {
            if owned.len() > 1 && owned.ends_with('/') {
                owned.pop();
            }
            std::borrow::Cow::Owned(owned)
        }
    }
}

#[derive(Debug)]
struct NoResponse;

impl std::fmt::Display for NoResponse {
    fn fmt(&self, _: &mut std::fmt::Formatter) -> std::fmt::Result {
        Ok(())
    }
}

impl std::error::Error for NoResponse {}
