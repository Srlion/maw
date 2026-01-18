use std::sync::LazyLock;

pub use http_body_util;
pub use hyper;

mod any_value_map;
mod app;
mod async_fn;
pub mod body_limit;
#[cfg(feature = "cookie")]
pub mod cookie;
#[cfg(feature = "csrf")]
pub mod csrf;
mod ctx;
mod error;
mod handler;
mod into_response;
#[cfg(feature = "logging")]
pub mod logging;
mod request;
mod response;
mod router;
#[cfg(feature = "session")]
pub mod session;
mod status_error;

pub fn all() -> http::Method {
    http::Method::from_bytes(b"*******").expect("failed to create ALL method") // should never happen
}

pub static ALL: LazyLock<http::Method> = LazyLock::new(all);

pub mod prelude {
    pub use crate::app::App;
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
#[cfg(feature = "session")]
pub use session::SessionConfig;
pub use tokio_util::sync::CancellationToken;
