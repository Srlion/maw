use std::{convert::Infallible, io};

#[derive(thiserror::Error, Debug)]
pub enum Error {
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
}

impl From<Infallible> for Error {
    fn from(_: Infallible) -> Self {
        unreachable!("Infallible can never occur")
    }
}
