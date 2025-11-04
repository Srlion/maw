use std::{collections::HashMap, sync::Arc};

use bytes::Bytes;
use http::{HeaderMap, HeaderValue, Method, Uri, Version, header::AsHeaderName};
use http_body_util::BodyExt;
use hyper::body::Incoming as IncomingBody;
use mime_guess::{Mime, mime};
use serde::{
    Deserialize,
    de::{DeserializeOwned, IntoDeserializer as _},
};
use smol_str::SmolStr;

use crate::{app::App, error::Error, locals::Locals};

pub struct Request {
    pub(crate) app: Arc<App>,
    pub(crate) parts: http::request::Parts,
    pub(crate) body: IncomingBody,
    pub params: HashMap<SmolStr, SmolStr>,
    pub(crate) locals: Locals,
    pub(crate) body_bytes: Option<Bytes>,
}

impl Request {
    #[inline]
    pub(crate) fn new(
        app: Arc<App>,
        request: http::Request<IncomingBody>,
        params: HashMap<SmolStr, SmolStr>,
    ) -> Self {
        let (parts, body) = request.into_parts();
        Request {
            app,
            parts,
            body,
            params,
            locals: Locals::new(),
            body_bytes: None,
        }
    }

    #[inline]
    pub fn app(&self) -> &App {
        &self.app
    }

    #[inline]
    pub fn params(&self) -> &HashMap<SmolStr, SmolStr> {
        &self.params
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
        self.params
            .get(key)
            .map(|v| v.as_str())
            .ok_or(Error::ParseNotFound)
            .and_then(|v| T::deserialize(v.into_deserializer()).map_err(Error::Parse))
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

    #[inline]
    pub fn locals(&self) -> &Locals {
        &self.locals
    }

    #[inline]
    pub fn locals_mut(&mut self) -> &mut Locals {
        &mut self.locals
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
        let limit = limit.unwrap_or_else(|| self.app.body_limit());
        let limited = http_body_util::Limited::new(&mut self.body, limit);
        let collected = limited.collect().await.map_err(Error::BodyCollect)?;
        let bytes = collected.to_bytes();
        self.body_bytes = Some(bytes);
        Ok(self.body_bytes.as_ref().unwrap())
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
