use std::sync::LazyLock;

pub use hyper;

mod app;
mod async_fn;
mod ctx;
mod error;
mod handler;
mod into_response;
mod locals;
mod request;
mod response;
mod router;
mod serializable_any;
mod status_error;

pub fn all() -> http::Method {
    http::Method::from_bytes(b"*******").expect("failed to create ALL method") // should never happen
}

pub static ALL: LazyLock<http::Method> = LazyLock::new(all);

pub mod prelude {
    pub use crate::app::App;
    pub use crate::ctx::Ctx;
    pub use crate::error::Error as MawError;
    pub use crate::router::Router;
    pub use crate::status_error::StatusError;
    pub use http::StatusCode;
    pub use http::method::Method;
    pub use minijinja;
}

pub use crate::into_response::IntoResponse;
