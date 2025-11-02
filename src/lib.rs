pub use hyper;

mod app;
mod async_fn;
mod ctx;
mod error;
mod handler;
mod locals;
mod request;
mod response;
mod router;
mod serializable_any;
mod status_error;

pub mod prelude {
    pub use crate::app::App;
    pub use crate::ctx::Ctx;
    pub use crate::error::Error as MawError;
    pub use crate::handler::HandlerOutput;
    pub use crate::router::Router;
    pub use crate::status_error::StatusError;
    pub use http::StatusCode;
    pub use http::method::Method;
    pub use minijinja;
}
