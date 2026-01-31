use std::sync::LazyLock;

pub use http_body_util;
pub use hyper;

mod any_value_map;
mod app;
mod async_fn;

mod ctx;
mod error;
mod handler;
mod into_response;
mod request;
mod response;
mod router;
mod status_error;

pub mod middlewares {
    #[cfg(feature = "middleware-cookie")]
    pub mod cookie;

    #[cfg(feature = "middleware-session")]
    pub(crate) mod session;

    #[cfg(feature = "middleware-csrf")]
    pub mod csrf;

    #[cfg(feature = "middleware-logging")]
    pub(crate) mod logging;

    #[cfg(feature = "middleware-catch_panic")]
    pub(crate) mod catch_panic;

    #[cfg(feature = "middleware-body_limit")]
    pub(crate) mod body_limit;
}

#[cfg(feature = "middleware-cookie")]
pub use middlewares::cookie::CookieMiddleware;

#[cfg(feature = "middleware-session")]
pub use middlewares::session::SessionMiddleware;

#[cfg(feature = "middleware-csrf")]
pub use middlewares::csrf::CsrfMiddleware;

#[cfg(feature = "middleware-logging")]
pub use middlewares::logging::LoggingMiddleware;

#[cfg(feature = "middleware-catch_panic")]
pub use middlewares::catch_panic::CatchPanicMiddleware;

#[cfg(feature = "middleware-body_limit")]
pub use middlewares::body_limit::BodyLimitMiddleware;

pub fn all() -> http::Method {
    http::Method::from_bytes(b"*******").expect("failed to create ALL method") // should never happen
}

pub static ALL: LazyLock<http::Method> = LazyLock::new(all);

pub mod prelude {
    pub use crate::app::App;
    pub use crate::async_fn::AsyncFn1 as Handler;
    pub use crate::ctx::Ctx;
    pub use crate::error::Error as MawError;
    pub use crate::router::{Router, WithState};
    pub use crate::status_error::StatusError;
    pub use http::StatusCode;
    pub use http::method::Method;
    #[cfg(feature = "minijinja")]
    pub use minijinja;
}

pub use crate::into_response::IntoResponse;
pub use crate::response::{BoxError, HttpBody};
pub use app::config::Config;
pub use serde_json;
pub use tokio_util::sync::CancellationToken;
