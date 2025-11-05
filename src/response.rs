use bytes::Bytes;
use http::{
    self, HeaderMap, HeaderName, HeaderValue, StatusCode,
    header::{self, InvalidHeaderName},
};
use http_body_util::Full;
use hyper::body::Body as _;
use minijinja::Value;
use std::{fmt, sync::Arc};

use crate::{app::App, error::Error, locals::Locals};

pub type HttpBody = Full<Bytes>;
pub type HttpResponse<T = HttpBody> = http::Response<T>;

pub struct Response {
    pub(crate) app: Arc<App>,
    pub(crate) inner: http::response::Response<HttpBody>,
    pub(crate) locals: Locals,
    // Indicates if the status code has been modified by the user
    pub(crate) status_modified: bool,
}

impl Response {
    #[inline]
    pub(crate) const fn from_response(app: Arc<App>, res: HttpResponse) -> Self {
        Response {
            app,
            inner: res,
            locals: Locals::new(),
            status_modified: false,
        }
    }

    pub fn app(&self) -> &App {
        &self.app
    }

    /// Sets the HTTP status for the response.
    pub fn status(&mut self, status: StatusCode) -> &mut Self {
        if !self.status_modified {
            self.status_modified = true;
        }
        *self.inner.status_mut() = status;
        self
    }

    /// Sets the status code and the correct status message in the body if the response body is **empty**.
    pub fn send_status(&mut self, status: StatusCode) -> &mut Self {
        self.status(status);

        if self.inner.body().size_hint().exact() == Some(0) {
            *self.inner.body_mut() = status.canonical_reason().unwrap_or("").into();
        }

        self
    }

    #[inline]
    pub fn headers(&self) -> &http::header::HeaderMap<HeaderValue> {
        self.inner.headers()
    }

    #[inline]
    pub fn headers_mut(&mut self) -> &mut http::header::HeaderMap<HeaderValue> {
        self.inner.headers_mut()
    }

    /// Sets multiple headers at once. Accepts:
    /// - A single tuple: `res.set(("Content-Type", "text/plain"))?`
    /// - An array of tuples: `res.set([("Content-Type", "text/plain"), ("ETag", "123")])?`
    /// - A Vec of tuples: `res.set(vec![...])?`
    /// - A HashMap: `res.set(hashmap)?`
    #[inline]
    pub fn set<H>(&mut self, headers: H) -> Result<&mut Self, Error>
    where
        H: SetIntoHeaders,
    {
        headers.into_headers(self.inner.headers_mut())?;
        Ok(self)
    }

    /// Appends a value to the HTTP response header field.
    /// If the header is not already set, it creates the header with the specified value.
    ///
    /// Examples:
    /// ```
    /// res.append("Link", "<http://localhost/>")?;
    /// res.append("Set-Cookie", ["foo=bar; Path=/", "bar=baz; HttpOnly"])?;
    /// res.append("Warning", vec!["199 Miscellaneous warning"])?;
    /// ```
    #[inline]
    pub fn append<K, V>(&mut self, key: K, values: V) -> Result<&mut Self, Error>
    where
        K: TryInto<HeaderName, Error = InvalidHeaderName>,
        V: AppendIntoHeaderValues,
        Error: From<K::Error>,
    {
        let key = key.try_into()?;
        values.append_to_header(self.inner.headers_mut(), key)?;
        Ok(self)
    }

    #[inline]
    pub fn send(&mut self, body: impl Into<HttpBody>) -> &mut Self {
        *self.inner.body_mut() = body.into();
        self
    }

    #[inline]
    pub fn content_type<V>(&mut self, value: V) -> Result<&mut Self, Error>
    where
        V: TryInto<HeaderValue>,
        Error: From<V::Error>,
    {
        self.set((header::CONTENT_TYPE, value))
    }

    #[inline]
    pub fn html(&mut self, s: &'static str) -> &mut Self {
        self.status(StatusCode::OK)
            .send(s)
            .content_type("text/html; charset=utf-8")
            .expect("failed to set Content-Type header");
        self
    }

    #[inline]
    pub fn locals(&self) -> &Locals {
        &self.locals
    }

    #[inline]
    pub fn locals_mut(&mut self) -> &mut Locals {
        &mut self.locals
    }

    #[inline]
    pub fn get_render_ctx(&self) -> Value {
        let mut ctx = minijinja::__context::make();
        for (key, value) in self.locals() {
            ctx.insert(key.into(), Value::from_serialize(value));
        }
        minijinja::__context::build(ctx)
    }

    #[inline]
    fn render_template(&mut self, template: &str, c: Value) -> &mut Self {
        let template = match self.app.render_env.get_template(template) {
            Ok(t) => t,
            Err(_) => {
                tracing::warn!("template not found: {}", template);
                return self.send_status(StatusCode::INTERNAL_SERVER_ERROR);
            }
        };

        let rendered = match template.render(&c) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("failed to render template {}: {}", template.name(), e);
                return self.send_status(StatusCode::INTERNAL_SERVER_ERROR);
            }
        };

        self.status(StatusCode::OK)
            .send(rendered)
            .content_type("text/html; charset=utf-8")
            .expect("failed to set Content-Type header");

        self
    }

    #[inline]
    pub fn render(&mut self, template: &str) -> &mut Self {
        let ctx = self.get_render_ctx();
        self.render_template(template, ctx)
    }

    #[inline]
    pub fn render_with(&mut self, template: &str, value: Value) -> &mut Self {
        let final_ctx = minijinja::context! {
            ..self.get_render_ctx(),
            ..value,
        };
        self.render_template(template, final_ctx)
    }
}

impl fmt::Debug for Response {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Response")
            .field("status_code", &self.inner.status())
            .field("body", &self.inner.body())
            .finish()
    }
}

pub trait SetIntoHeaders {
    fn into_headers(self, map: &mut HeaderMap) -> Result<(), Error>;
}

impl<K, V> SetIntoHeaders for (K, V)
where
    K: TryInto<HeaderName>,
    V: TryInto<HeaderValue>,
    Error: From<K::Error> + From<V::Error>,
{
    fn into_headers(self, map: &mut HeaderMap) -> Result<(), Error> {
        let k = self.0.try_into()?;
        let v = self.1.try_into()?;
        map.insert(k, v);
        Ok(())
    }
}

impl<K, V, const N: usize> SetIntoHeaders for [(K, V); N]
where
    K: TryInto<HeaderName>,
    V: TryInto<HeaderValue>,
    Error: From<K::Error> + From<V::Error>,
{
    fn into_headers(self, map: &mut HeaderMap) -> Result<(), Error> {
        for (key, value) in self {
            let k = key.try_into()?;
            let v = value.try_into()?;
            map.insert(k, v);
        }
        Ok(())
    }
}

impl<K, V> SetIntoHeaders for Vec<(K, V)>
where
    K: TryInto<HeaderName>,
    V: TryInto<HeaderValue>,
    Error: From<K::Error> + From<V::Error>,
{
    fn into_headers(self, map: &mut HeaderMap) -> Result<(), Error> {
        for (key, value) in self {
            let k = key.try_into()?;
            let v = value.try_into()?;
            map.insert(k, v);
        }
        Ok(())
    }
}

pub trait AppendIntoHeaderValues {
    fn append_to_header(self, map: &mut HeaderMap, key: HeaderName) -> Result<(), Error>;
}

impl AppendIntoHeaderValues for &str {
    fn append_to_header(self, map: &mut HeaderMap, key: HeaderName) -> Result<(), Error> {
        let value = HeaderValue::try_from(self).map_err(|e| Error::from(http::Error::from(e)))?;
        map.append(key, value);
        Ok(())
    }
}

impl AppendIntoHeaderValues for String {
    fn append_to_header(self, map: &mut HeaderMap, key: HeaderName) -> Result<(), Error> {
        let value = HeaderValue::try_from(self).map_err(|e| Error::from(http::Error::from(e)))?;
        map.append(key, value);
        Ok(())
    }
}

impl AppendIntoHeaderValues for HeaderValue {
    fn append_to_header(self, map: &mut HeaderMap, key: HeaderName) -> Result<(), Error> {
        map.append(key, self);
        Ok(())
    }
}

impl AppendIntoHeaderValues for &[&str] {
    fn append_to_header(self, map: &mut HeaderMap, key: HeaderName) -> Result<(), Error> {
        for &value in self {
            let v = HeaderValue::try_from(value).map_err(|e| Error::from(http::Error::from(e)))?;
            map.append(key.clone(), v);
        }
        Ok(())
    }
}

impl AppendIntoHeaderValues for &[String] {
    fn append_to_header(self, map: &mut HeaderMap, key: HeaderName) -> Result<(), Error> {
        for value in self {
            let v = HeaderValue::try_from(value.as_str())
                .map_err(|e| Error::from(http::Error::from(e)))?;
            map.append(key.clone(), v);
        }
        Ok(())
    }
}

impl AppendIntoHeaderValues for Vec<String> {
    fn append_to_header(self, map: &mut HeaderMap, key: HeaderName) -> Result<(), Error> {
        for value in self {
            let v = HeaderValue::try_from(value).map_err(|e| Error::from(http::Error::from(e)))?;
            map.append(key.clone(), v);
        }
        Ok(())
    }
}

impl<const N: usize> AppendIntoHeaderValues for [String; N] {
    fn append_to_header(self, map: &mut HeaderMap, key: HeaderName) -> Result<(), Error> {
        for value in self {
            let v = HeaderValue::try_from(value).map_err(|e| Error::from(http::Error::from(e)))?;
            map.append(key.clone(), v);
        }
        Ok(())
    }
}

impl<const N: usize> AppendIntoHeaderValues for [&str; N] {
    fn append_to_header(self, map: &mut HeaderMap, key: HeaderName) -> Result<(), Error> {
        for value in self {
            let v = HeaderValue::try_from(value).map_err(|e| Error::from(http::Error::from(e)))?;
            map.append(key.clone(), v);
        }
        Ok(())
    }
}

impl<const N: usize> AppendIntoHeaderValues for &[String; N] {
    fn append_to_header(self, map: &mut HeaderMap, key: HeaderName) -> Result<(), Error> {
        for value in self {
            let v = HeaderValue::try_from(value).map_err(|e| Error::from(http::Error::from(e)))?;
            map.append(key.clone(), v);
        }
        Ok(())
    }
}

impl<const N: usize> AppendIntoHeaderValues for &[&str; N] {
    fn append_to_header(self, map: &mut HeaderMap, key: HeaderName) -> Result<(), Error> {
        for value in self {
            let v = HeaderValue::try_from(*value).map_err(|e| Error::from(http::Error::from(e)))?;
            map.append(key.clone(), v);
        }
        Ok(())
    }
}

impl AppendIntoHeaderValues for Vec<&str> {
    fn append_to_header(self, map: &mut HeaderMap, key: HeaderName) -> Result<(), Error> {
        for value in self {
            let v = HeaderValue::try_from(value).map_err(|e| Error::from(http::Error::from(e)))?;
            map.append(key.clone(), v);
        }
        Ok(())
    }
}
