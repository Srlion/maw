use std::error::Error as StdError;
use std::{convert::Infallible, io};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("data does not exist")]
    ParseNotFound,

    #[error("failed to parse: {0}")]
    Parse(#[from] serde::de::value::Error),

    #[error("io: {0}")]
    Io(#[from] io::Error),

    #[error("http: {0}")]
    Http(#[from] http::Error),

    #[error("matchit: {0}")]
    MatchitInsert(#[from] matchit::InsertError),

    #[error("failed to parse address")]
    FailedToParseAddr,

    #[error("invalid header name: {0}")]
    InvalidHeaderName(#[from] http::header::InvalidHeaderName),

    #[error("invalid header value: {0}")]
    InvalidHeaderValue(#[from] http::header::InvalidHeaderValue),

    #[error("{0}")]
    BodyCollect(Box<dyn StdError + Send + Sync>),

    #[error("serde_json: {0}")]
    SerdeJson(#[from] serde_json::Error),

    #[error("Utf8 error: {0}")]
    Utf8(#[from] std::str::Utf8Error),

    #[error("unsupported media type")]
    UnsupportedMediaType,

    #[error("missing content type")]
    MissingContentType,

    #[cfg(feature = "xml")]
    #[error("quick-xml de error: {0}")]
    QuickXmlDe(#[from] quick_xml::DeError),

    #[error("minijinja error: {0}")]
    MiniJinja(#[from] minijinja::Error),
}

impl From<Infallible> for Error {
    fn from(_: Infallible) -> Self {
        panic!("Infallible can never be constructed")
    }
}
