use std::convert::Infallible;
use std::future::Future;
use std::net;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use http::StatusCode;
use hyper::server::conn::http1;
use hyper::{Request as HyperRequest, body::Incoming as IncomingBody};
use hyper_util::rt::TokioIo;
use minijinja::path_loader;
use smallvec::SmallVec;
use smol_str::SmolStr;
use tokio::net::TcpListener;

pub(crate) mod config;

use crate::ALL;
use crate::locals::Locals;
use crate::request::Request;
use crate::response::Response;
use crate::{
    error::Error,
    router::{self, MatchRouter},
};

type HttpBody = http_body_util::Full<bytes::Bytes>;
type HttpResponse<T = HttpBody> = http::Response<T>;

#[derive(Default)]
pub struct App {
    pub(crate) router: router::Router,
    pub(crate) render_env: minijinja::Environment<'static>,
    pub(crate) locals: Mutex<Locals>,
    pub(crate) built_router: MatchRouter,
    pub(crate) config: config::Config,
}

impl App {
    pub fn new() -> Self {
        Self::default()
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

    pub fn views(mut self, path: impl AsRef<std::path::Path>) -> Self {
        self.render_env.set_loader(path_loader(path));
        self
    }

    /// Provides access to the application locals.
    pub fn with_locals<F>(&self, f: F) -> &Self
    where
        F: FnOnce(&Locals),
    {
        let locals = self.locals.lock().unwrap();
        f(&locals);
        self
    }

    /// Provides mutable access to the application locals.
    pub fn with_locals_mut<F>(&self, f: F) -> &Self
    where
        F: FnOnce(&mut Locals),
    {
        let mut locals = self.locals.lock().unwrap();
        f(&mut locals);
        self
    }

    pub async fn listen<A>(mut self, addr: A) -> Result<(), Error>
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
        let mut signal = std::pin::pin!(shutdown_signal());

        loop {
            tokio::select! {
                Ok((stream, peer_addr)) = listener.accept() => {
                    let io = TokioIo::new(stream);
                    let conn = http.serve_connection(io, ConnectionHandler { app: arc_app.clone(), peer_addr });
                    // Watch this connection
                    let fut = graceful.watch(conn);

                    tokio::task::spawn(async move {
                        if let Err(e) = fut.await {
                            tracing::trace!("connection failed: {e:?}");
                        } else {
                            tracing::trace!("connection successful");
                        }
                    });
                }
                _ = &mut signal => {
                    drop(listener);
                    tracing::info!("Graceful shutdown signal received");
                    break;
                }
            }
        }

        tracing::info!("Waiting for all connections to close...");
        tokio::select! {
            _ = graceful.shutdown() => {
               tracing::info!("All connections gracefully closed");
            },
            _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => {
               tracing::info!("Timed out waiting for all connections to close");
            }
        }

        Ok(())
    }
}

impl Clone for App {
    fn clone(&self) -> Self {
        App {
            router: self.router.clone(),
            render_env: self.render_env.clone(),
            locals: Mutex::new(self.locals.lock().unwrap().clone()),
            built_router: MatchRouter::default(),
            config: self.config.clone(),
        }
    }
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C signal handler");
}

async fn handle_request(
    request: HyperRequest<IncomingBody>,
    app: Arc<App>,
    peer_addr: net::SocketAddr,
) -> Result<HttpResponse, Infallible> {
    let mut response = HttpResponse::new(HttpBody::default());

    let path = handle_path_slashes(request.uri().path().as_bytes());
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

    let mut ctx = crate::ctx::Ctx::new(req, res, handlers);
    ctx.next().await;

    if ctx.req.method() == http::Method::HEAD {
        *ctx.res.inner.body_mut() = HttpBody::default();
    }

    Ok(ctx.res.inner)
}

fn handle_path_slashes(path_bytes: &[u8]) -> SmolStr {
    let mut result: SmallVec<[u8; 24]> = SmallVec::new();
    let mut last_was_slash = false;

    for &byte in path_bytes {
        match byte {
            b'/' | b'\\' => {
                if !last_was_slash {
                    result.push(b'/');
                }
                last_was_slash = true;
            }
            _ => {
                // Convert uppercase ASCII directly on bytes
                let byte = if byte.is_ascii_uppercase() {
                    byte + 32 // Convert to lowercase
                } else {
                    byte
                };
                result.push(byte);
                last_was_slash = false;
            }
        }
    }

    // SAFETY: path was constructed from valid UTF-8 bytes
    let path = unsafe { std::str::from_utf8_unchecked(&result) };
    SmolStr::new(path)
}

struct ConnectionHandler {
    app: Arc<App>,
    // The peer address of the connection
    peer_addr: std::net::SocketAddr,
}

impl hyper::service::Service<HyperRequest<IncomingBody>> for ConnectionHandler {
    type Response = HttpResponse;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    #[inline]
    fn call(&self, req: HyperRequest<IncomingBody>) -> Self::Future {
        Box::pin(handle_request(req, self.app.clone(), self.peer_addr))
    }
}
