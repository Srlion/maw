use std::{collections::HashMap, sync::Arc};

use bytes::Bytes;
use http::{HeaderMap, HeaderValue, Method, Uri, Version, header::AsHeaderName};
use http_body_util::BodyExt;
use hyper::body::Incoming as IncomingBody;
use mime_guess::{Mime, mime};
use serde::de::DeserializeOwned;
use smol_str::SmolStr;

use crate::{
    any_map::{AnyMap, CloneableAny},
    app::App,
    prelude::StatusError,
};

pub struct Request {
    pub(crate) app: Arc<App>,
    pub(crate) parts: http::request::Parts,
    pub(crate) body: IncomingBody,
    pub params: HashMap<SmolStr, SmolStr>,
    pub locals: AnyMap<dyn CloneableAny>,
    pub(crate) cached_body: Option<Bytes>,
    pub(crate) ip: std::net::SocketAddr,
    /// Max body size that the server accepts.
    ///
    /// Default: 4MB
    pub(crate) body_limit: usize,
}

impl Request {
    #[inline]
    pub(crate) fn new(
        app: Arc<App>,
        request: http::Request<IncomingBody>,
        params: HashMap<SmolStr, SmolStr>,
        peer_addr: std::net::SocketAddr,
    ) -> Self {
        let (parts, body) = request.into_parts();
        let body_limit = app.body_limit;
        Request {
            app,
            parts,
            body,
            params,
            locals: AnyMap::new(),
            cached_body: None,
            ip: peer_addr,
            body_limit,
        }
    }

    #[inline]
    pub fn app(&self) -> &App {
        &self.app
    }

    /// Get a path parameter and deserialize it into type `T`.
    ///
    /// Works with any type implementing `DeserializeOwned`: primitives,
    /// `String`, enums, newtypes, etc.
    ///
    /// For borrowing as `&str` without allocation, use [`param_str`](Self::param_str).
    ///
    /// # Examples
    ///
    /// ```
    /// let id: u32 = req.param("id")?;
    /// let name: String = req.param("name")?;
    /// ```
    #[inline]
    pub fn param<T>(&self, key: &str) -> Result<T, ParamError>
    where
        T: DeserializeOwned,
    {
        let value = self
            .params
            .get(key)
            .ok_or(ParamError::Missing(key.into()))?;
        serde_plain::from_str(value.as_str()).map_err(|e| ParamError::Invalid {
            key: key.into(),
            value: value.clone(),
            source: e,
        })
    }

    /// Get param as &str.
    ///
    /// Returns empty string if not found.
    #[inline]
    pub fn param_str(&self, key: &str) -> &str {
        match self.params.get(key) {
            Some(v) => v.as_str(),
            None => "",
        }
    }

    #[inline]
    pub fn method(&self) -> &Method {
        &self.parts.method
    }

    #[inline]
    pub fn uri(&self) -> &Uri {
        &self.parts.uri
    }

    #[inline]
    pub fn headers(&self) -> &HeaderMap<HeaderValue> {
        &self.parts.headers
    }

    #[inline]
    pub fn headers_mut(&mut self) -> &mut HeaderMap<HeaderValue> {
        &mut self.parts.headers
    }

    /// Returns the specified Header value as a &str.
    #[inline]
    pub fn header<K>(&self, key: K) -> Option<&str>
    where
        K: AsHeaderName,
    {
        let v = self.headers().get(key);
        match v {
            Some(v) => Some(match v.to_str() {
                Ok(v) => v,
                Err(_) => return None,
            }),
            None => None,
        }
    }

    #[inline]
    pub fn version(&self) -> Version {
        self.parts.version
    }

    /// Set the maximum body size that the request will read.
    ///
    /// Default: 4MB
    pub fn set_body_limit(&mut self, limit: usize) {
        self.body_limit = limit;
    }

    /// Get raw body bytes.
    ///
    /// If body has already been read, returns the cached bytes. (Limits are not re-applied.)
    ///
    /// Default limit is 4MB.
    #[inline]
    pub async fn bytes(&mut self) -> Result<&Bytes, BodyError> {
        if let Some(ref bytes) = self.cached_body {
            return Ok(bytes);
        }
        let limit = self.body_limit;
        let limited = http_body_util::Limited::new(&mut self.body, limit);
        let collected = limited.collect().await.map_err(BodyError::Collect)?;
        let bytes = collected.to_bytes();
        self.cached_body = Some(bytes);
        Ok(self.cached_body.as_ref().unwrap())
    }

    /// Get body as text.
    ///
    /// If body has already been read, returns the cached bytes. (Limits are not re-applied.)
    ///
    /// Default limit is 4MB.
    #[inline]
    pub async fn text(&mut self) -> Result<&str, BodyError> {
        let bytes = self.bytes().await?;
        let s = std::str::from_utf8(bytes)?;
        Ok(s)
    }

    #[inline]
    pub async fn json<T: DeserializeOwned>(&mut self) -> Result<T, ParseError> {
        let bytes = self.bytes().await?;
        Ok(serde_json::from_slice(bytes)?)
    }

    #[inline]
    pub async fn form<T: DeserializeOwned>(&mut self) -> Result<T, ParseError> {
        let bytes = self.bytes().await?;
        Ok(serde_urlencoded::from_bytes(bytes)?)
    }

    #[cfg(feature = "xml")]
    #[inline]
    pub async fn xml<T: DeserializeOwned>(&mut self) -> Result<T, ParseError> {
        let bytes = self.bytes().await?;
        let str = std::str::from_utf8(bytes)?;
        Ok(quick_xml::de::from_str(str)?)
    }

    /// Parse body based on content type.
    ///
    /// If body has already been read, returns the cached bytes. (Limits are not re-applied.)
    ///
    /// # Errors
    /// Returns `Error::UnsupportedMediaType` if the content type is not supported.
    ///
    /// Returns `Error::MissingContentType` if the content type header is missing.
    ///
    /// Default limit is 4MB.
    #[inline]
    pub async fn parse<T: DeserializeOwned>(&mut self) -> Result<T, ParseError> {
        match self.content_type() {
            Some(mime) => {
                if mime.suffix() == Some(mime::JSON) || mime.subtype() == mime::JSON {
                    self.json().await
                } else if mime.type_() == mime::APPLICATION {
                    match mime.subtype().as_str() {
                        "x-www-form-urlencoded" => self.form().await,
                        #[cfg(feature = "xml")]
                        "xml" => self.xml().await,
                        _ => Err(BodyError::UnsupportedMediaType.into()),
                    }
                } else if mime.type_() == mime::TEXT && mime.subtype() == mime::XML {
                    #[cfg(feature = "xml")]
                    {
                        self.xml().await
                    }
                    #[cfg(not(feature = "xml"))]
                    {
                        Err(BodyError::UnsupportedMediaType.into())
                    }
                } else {
                    Err(BodyError::UnsupportedMediaType.into())
                }
            }
            None => Err(BodyError::MissingContentType.into()),
        }
    }

    /// Get content type.
    #[inline]
    pub fn content_type(&self) -> Option<Mime> {
        self.headers()
            .get(http::header::CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
            .and_then(|v| v.parse().ok())
    }

    #[inline]
    pub fn size_hint(&self) -> hyper::body::SizeHint {
        hyper::body::Body::size_hint(&self.body)
    }

    #[inline]
    fn extract_ip_from_header(&self, header_name: &str) -> String {
        self.header(header_name).unwrap_or_default().to_string()
    }

    #[inline]
    pub fn ip(&self) -> String {
        if let Some(ref header_name) = self.app.proxy_header {
            return self.extract_ip_from_header(header_name);
        }
        self.ip.to_string()
    }

    #[inline]
    pub fn is_local(&self) -> bool {
        self.ip.ip().is_loopback()
    }

    #[inline]
    pub fn query<T: DeserializeOwned>(&self) -> Result<T, QueryError> {
        let qs = self.parts.uri.query().unwrap_or("");
        Ok(serde_urlencoded::from_str(qs)?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ParamError {
    #[error("Missing path parameter: {0}")]
    Missing(SmolStr),

    #[error("Invalid value for path parameter: {key}")]
    Invalid {
        key: SmolStr,
        value: SmolStr,
        #[source]
        source: serde_plain::Error,
    },
}

impl From<ParamError> for StatusError {
    fn from(e: ParamError) -> Self {
        match e {
            ParamError::Missing(e) => {
                StatusError::bad_request().brief(format!("Missing path parameter: {e}"))
            }
            ParamError::Invalid { key, .. } => StatusError::unprocessable_entity()
                .brief(format!("Invalid value for path parameter: {key}")),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BodyError {
    #[error("Failed to collect body")]
    Collect(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("Body is not valid UTF-8")]
    InvalidUtf8(#[from] std::str::Utf8Error),

    #[error("Missing Content-Type header")]
    MissingContentType,

    #[error("Unsupported media type")]
    UnsupportedMediaType,
}

impl From<BodyError> for StatusError {
    fn from(e: BodyError) -> Self {
        match e {
            BodyError::Collect(_) => StatusError::bad_request().brief("Failed to read body"),
            BodyError::InvalidUtf8(_) => {
                StatusError::bad_request().brief("Body is not valid UTF-8")
            }
            BodyError::MissingContentType => {
                StatusError::bad_request().brief("Missing Content-Type header")
            }
            BodyError::UnsupportedMediaType => {
                StatusError::unsupported_media_type().brief("Unsupported media type")
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Failed to parse JSON")]
    Json(#[from] serde_json::Error),

    #[error("Failed to parse form data")]
    Form(#[from] serde_urlencoded::de::Error),

    #[cfg(feature = "xml")]
    #[error("Failed to parse XML")]
    Xml(#[from] quick_xml::DeError),

    #[error("Body is not valid UTF-8")]
    InvalidUtf8(#[from] std::str::Utf8Error),

    #[error(transparent)]
    Body(#[from] BodyError),
}

impl From<ParseError> for StatusError {
    fn from(e: ParseError) -> Self {
        match e {
            ParseError::Json(ref err) => {
                if err.is_syntax() || err.is_eof() {
                    StatusError::bad_request().brief("Invalid JSON syntax")
                } else {
                    StatusError::unprocessable_entity()
                        .brief("Failed to deserialize JSON into expected type")
                }
            }
            ParseError::Form(_) => StatusError::bad_request().brief("Invalid form data"),
            #[cfg(feature = "xml")]
            ParseError::Xml(ref err) => {
                use quick_xml::de::DeError;
                match err {
                    DeError::InvalidXml(_)
                    | DeError::UnexpectedEof
                    | DeError::UnexpectedStart(_) => {
                        StatusError::bad_request().brief("Invalid XML syntax")
                    }
                    DeError::Custom(_) => StatusError::unprocessable_entity()
                        .brief("Failed to deserialize XML into expected type"),
                    _ => StatusError::bad_request().brief("Failed to parse XML"),
                }
            }
            ParseError::InvalidUtf8(_) => {
                StatusError::bad_request().brief("Body is not valid UTF-8")
            }
            ParseError::Body(b) => b.into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum QueryError {
    #[error("Failed to parse query string")]
    Parse(#[from] serde_urlencoded::de::Error),
}

impl From<QueryError> for StatusError {
    fn from(_: QueryError) -> Self {
        StatusError::bad_request().brief("Invalid query string")
    }
}
