use std::{collections::HashMap, sync::Arc};

use bytes::Bytes;
use http::{HeaderMap, HeaderValue, Method, Uri, Version, header::AsHeaderName};
use http_body_util::BodyExt;
use hyper::body::Incoming as IncomingBody;
use mime_guess::{Mime, mime};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde::de::value::BorrowedStrDeserializer;
use smol_str::SmolStr;

use crate::{app::App, error::Error, locals::Locals};

pub struct Request {
    pub(crate) app: Arc<App>,
    pub(crate) parts: http::request::Parts,
    pub(crate) body: IncomingBody,
    pub params: HashMap<SmolStr, SmolStr>,
    pub locals: Locals,
    pub(crate) body_bytes: Option<Bytes>,
    pub(crate) ip: std::net::SocketAddr,
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
        Request {
            app,
            parts,
            body,
            params,
            locals: Locals::new(),
            body_bytes: None,
            ip: peer_addr,
        }
    }

    #[inline]
    pub fn app(&self) -> &App {
        &self.app
    }

    #[inline]
    pub fn param<'a, T>(&'a self, key: &str) -> Option<T>
    where
        T: Deserialize<'a>,
    {
        self.try_param(key).ok()
    }

    #[inline]
    pub fn try_param<'a, T>(&'a self, key: &str) -> Result<T, Error>
    where
        T: Deserialize<'a>,
    {
        let value = self.params.get(key).ok_or(Error::ParseNotFound)?;
        T::deserialize(BorrowedStrDeserializer::new(value.as_str())).map_err(Error::Parse)
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
    pub fn get<K>(&self, key: K) -> Option<&str>
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

    /// Get raw body bytes.
    ///
    /// If body has already been read, returns the cached bytes. (Limits are not re-applied.)
    ///
    /// Default limit is 4MB.
    #[inline]
    pub async fn body_raw(&mut self, limit: Option<usize>) -> Result<&Bytes, Error> {
        if let Some(ref bytes) = self.body_bytes {
            return Ok(bytes);
        }
        let limit = limit.unwrap_or_else(|| self.app.config.body_limit);
        let limited = http_body_util::Limited::new(&mut self.body, limit);
        let collected = limited.collect().await.map_err(Error::BodyCollect)?;
        let bytes = collected.to_bytes();
        self.body_bytes = Some(bytes);
        Ok(self.body_bytes.as_ref().unwrap())
    }

    /// Get body as text.
    ///
    /// If body has already been read, returns the cached bytes. (Limits are not re-applied.)
    ///
    /// Default limit is 4MB.
    #[inline]
    pub async fn body_text(&mut self, limit: Option<usize>) -> Result<&str, Error> {
        let bytes = self.body_raw(limit).await?;
        let s = std::str::from_utf8(bytes)?;
        Ok(s)
    }

    #[inline]
    pub async fn parse_json<T: DeserializeOwned>(
        &mut self,
        limit: Option<usize>,
    ) -> Result<T, Error> {
        let bytes = self.body_raw(limit).await?;
        let value: T = serde_json::from_slice(bytes)?;
        Ok(value)
    }

    #[inline]
    pub async fn parse_form<T: DeserializeOwned>(
        &mut self,
        limit: Option<usize>,
    ) -> Result<T, Error> {
        let bytes = self.body_raw(limit).await?;
        let value: T = serde_urlencoded::from_bytes(bytes)?;
        Ok(value)
    }

    #[inline]
    pub async fn parse_xml<T: DeserializeOwned>(
        &mut self,
        limit: Option<usize>,
    ) -> Result<T, Error> {
        let bytes = self.body_raw(limit).await?;
        let str = std::str::from_utf8(bytes)?;
        let value: T = quick_xml::de::from_str(str)?;
        Ok(value)
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
    pub async fn parse_body<T: DeserializeOwned>(
        &mut self,
        limit: Option<usize>,
    ) -> Result<T, Error> {
        match self.content_type() {
            Some(mime) => {
                if mime.suffix() == Some(mime::JSON) || mime.subtype() == mime::JSON {
                    self.parse_json(limit).await
                } else if mime.type_() == mime::APPLICATION {
                    match mime.subtype().as_str() {
                        "x-www-form-urlencoded" => self.parse_form(limit).await,
                        "xml" => self.parse_xml(limit).await,
                        _ => Err(Error::UnsupportedMediaType),
                    }
                } else if mime.type_() == mime::TEXT && mime.subtype() == mime::XML {
                    self.parse_xml(limit).await
                } else {
                    Err(Error::UnsupportedMediaType)
                }
            }
            None => Err(Error::MissingContentType),
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
        self.get(header_name).unwrap_or_default().to_string()
    }

    #[inline]
    pub fn ip(&self) -> String {
        if !self.app.config.proxy_header.is_empty() {
            return self.extract_ip_from_header(&self.app.config.proxy_header);
        }
        self.ip.to_string()
    }

    #[inline]
    pub fn is_from_local(&self) -> bool {
        self.ip.ip().is_loopback()
    }

    #[inline]
    pub fn query(&self) -> HashMap<String, String> {
        let query_string = self.parts.uri.query().unwrap_or("");
        serde_urlencoded::from_str(query_string).unwrap_or_default()
    }

    #[inline]
    pub fn query_parse<T: DeserializeOwned>(&self) -> Result<T, Error> {
        let query_string = self.parts.uri.query().unwrap_or("");
        serde_urlencoded::from_str(query_string).map_err(Error::from)
    }

    // #[inline]
    // pub async fn body(&mut self) -> Result<Bytes, Infallible> {
    // 	self.body_with_size(64 * 1024).await
    // }

    // #[inline]
    // pub async fn body_with_size(&mut self, limit: usize) -> Result<Bytes, Infallible> {
    // 	match std::mem::replace(&mut self.body, None) {
    // 		Some(body) => match LimitBody::new(body, limit).collect().await {
    // 			Ok(collected) => Ok(collected.to_bytes()),
    // 			Err(_) => {
    // 				// bail!("failed to collect body");
    // 				Ok(Bytes::new())
    // 			}
    // 		},
    // 		None => {
    // 			// bail!("body already consumed");
    // 			Ok(Bytes::new())
    // 		}
    // 	}
    // }
}
