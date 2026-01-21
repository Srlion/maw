use std::convert::Infallible;
use std::net;
use std::sync::{Arc, RwLock};

use http::StatusCode;
use hyper::server::conn::http1;
use hyper::{Request as HyperRequest, body::Incoming as IncomingBody};
use hyper_util::rt::TokioIo;
use smol_str::SmolStr;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

pub(crate) mod config;

use crate::ALL;
use crate::any_value_map::{AnyMap, SerializableAny};
use crate::request::Request;
use crate::response::{HttpBody, Response};
use crate::{
    error::Error,
    router::{self, MatchRouter},
};

type HttpResponse = http::Response<HttpBody>;

pub struct App {
    pub(crate) router: router::Router,
    #[cfg(feature = "minijinja")]
    pub(crate) render_env: minijinja::Environment<'static>,
    pub(crate) locals: RwLock<AnyMap<dyn SerializableAny>>,
    pub(crate) built_router: MatchRouter,
    pub(crate) config: config::Config,
    /// Shutdown timeout in seconds
    pub(crate) shutdown_timeout: std::time::Duration,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        App {
            router: router::Router::new(),
            #[cfg(feature = "minijinja")]
            render_env: minijinja::Environment::new(),
            locals: RwLock::new(AnyMap::new()),
            built_router: MatchRouter::default(),
            config: config::Config::default(),
            shutdown_timeout: std::time::Duration::from_secs(10),
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

    #[cfg(feature = "minijinja")]
    pub fn views(mut self, path: impl AsRef<std::path::Path>) -> Self {
        self.render_env.set_loader(minijinja::path_loader(path));
        self
    }

    #[cfg(feature = "minijinja")]
    pub fn render_env_filter<N, F, Rv, Args>(mut self, name: N, f: F) -> Self
    where
        N: Into<std::borrow::Cow<'static, str>>,
        F: minijinja::functions::Function<Rv, Args>,
        Rv: minijinja::value::FunctionResult,
        Args: for<'a> minijinja::value::FunctionArgs<'a>,
    {
        self.render_env.add_filter(name.into(), f);
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
    pub fn with_locals<F>(&self, f: F) -> &Self
    where
        F: FnOnce(&AnyMap<dyn SerializableAny>),
    {
        let locals = self.locals.read().unwrap();
        f(&locals);
        self
    }

    /// Provides mutable access to the application locals.
    pub fn with_locals_mut<F>(&self, f: F) -> &Self
    where
        F: FnOnce(&mut AnyMap<dyn SerializableAny>),
    {
        let mut locals = self.locals.write().unwrap();
        f(&mut locals);
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
        self.built_router = self.router.build()?;
        let arc_app = Arc::new(self);

        let addr = addr
            .to_socket_addrs()?
            .next()
            .ok_or(Error::FailedToParseAddr)?;

        let listener = TcpListener::bind(addr).await?;
        tracing::info!("Http app listening on http://{}", addr);

        let http = http1::Builder::new();
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

                    let conn = http.serve_connection(io, service);
                    let fut = graceful.watch(conn);
                    tokio::task::spawn(async move {
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
            router: self.router.clone(),
            #[cfg(feature = "minijinja")]
            render_env: self.render_env.clone(),
            locals: RwLock::new(self.locals.read().unwrap().clone()),
            built_router: MatchRouter::default(),
            config: self.config.clone(),
            shutdown_timeout: self.shutdown_timeout,
        }
    }
}

async fn handle_request(
    request: HyperRequest<IncomingBody>,
    app: Arc<App>,
    peer_addr: net::SocketAddr,
) -> Result<HttpResponse, Infallible> {
    let mut response = HttpResponse::new(HttpBody::default());

    let path = request.uri().path();
    let matched_route = match app.built_router.at(path) {
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

    if c.req.method() == http::Method::HEAD {
        *c.res.inner.body_mut() = HttpBody::default();
    }

    Ok(c.res.inner)
}
