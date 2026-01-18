use std::{
    error::Error as StdError,
    fmt,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use bytes::Bytes;
use futures_core::Stream;
use http::{
    self, HeaderMap, HeaderName, HeaderValue, StatusCode,
    header::{self, InvalidHeaderName},
};
use http_body::{Body as HttpBodyTrait, Frame, SizeHint};
use http_body_util::Full;

use crate::{
    any_value_map::{AnyMap, SerializableAny},
    app::App,
    error::Error,
};

pub type BoxError = Box<dyn StdError + Send + Sync>;

pub enum StreamKind {
    /// Stream produces raw bytes (wrapped into data frames automatically)
    Bytes(Pin<Box<dyn Stream<Item = Result<Bytes, BoxError>> + Send + Sync>>),
    /// Stream produces frames directly (can include trailers)
    Frames(Pin<Box<dyn Stream<Item = Result<Frame<Bytes>, BoxError>> + Send + Sync>>),
}

#[derive(Default)]
pub enum HttpBody {
    #[default]
    Empty,
    Full(Full<Bytes>),
    Stream(StreamKind),
}

impl fmt::Debug for HttpBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HttpBody::Empty => f.debug_struct("HttpBody::Empty").finish(),
            HttpBody::Full(b) => f.debug_struct("HttpBody::Full").field("body", b).finish(),
            HttpBody::Stream(_) => f.debug_struct("HttpBody::Stream").finish(),
        }
    }
}

impl HttpBody {
    pub fn full(bytes: Bytes) -> Self {
        HttpBody::Full(Full::new(bytes))
    }

    /// Create a stream that produces data frames only
    pub fn stream<S>(stream: S) -> Self
    where
        S: Stream<Item = Result<Bytes, BoxError>> + Send + Sync + 'static,
    {
        HttpBody::Stream(StreamKind::Bytes(Box::pin(stream)))
    }

    /// Create a stream that can produce both data and trailer frames
    pub fn stream_frames<S>(stream: S) -> Self
    where
        S: Stream<Item = Result<Frame<Bytes>, BoxError>> + Send + Sync + 'static,
    {
        HttpBody::Stream(StreamKind::Frames(Box::pin(stream)))
    }
}

impl HttpBodyTrait for HttpBody {
    type Data = Bytes;
    type Error = BoxError;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.get_mut();
        match this {
            HttpBody::Empty => Poll::Ready(None),
            HttpBody::Full(full) => Pin::new(full)
                .poll_frame(cx)
                .map(|opt| opt.map(|res| res.map_err(|never| match never {}))),
            HttpBody::Stream(kind) => match kind {
                // Wrap bytes into data frames
                StreamKind::Bytes(stream) => match stream.as_mut().poll_next(cx) {
                    Poll::Ready(Some(Ok(bytes))) => Poll::Ready(Some(Ok(Frame::data(bytes)))),
                    Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
                    Poll::Ready(None) => Poll::Ready(None),
                    Poll::Pending => Poll::Pending,
                },
                // Pass frames through directly
                StreamKind::Frames(stream) => stream.as_mut().poll_next(cx),
            },
        }
    }

    fn size_hint(&self) -> SizeHint {
        match self {
            HttpBody::Empty => SizeHint::with_exact(0),
            HttpBody::Full(full) => full.size_hint(),
            HttpBody::Stream(_) => SizeHint::new(),
        }
    }
}

pub type HttpResponse<T = HttpBody> = http::Response<T>;

pub struct Response {
    pub(crate) app: Arc<App>,
    pub(crate) inner: http::response::Response<HttpBody>,
    pub locals: AnyMap<dyn SerializableAny>,
    // Indicates if the status code has been modified by the user
    pub(crate) status_modified: bool,
}

impl Response {
    #[inline]
    pub(crate) const fn from_response(app: Arc<App>, res: HttpResponse) -> Self {
        Response {
            app,
            inner: res,
            locals: AnyMap::new(),
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
            let text = status.canonical_reason().unwrap_or("").to_string();
            *self.inner.body_mut() = HttpBody::full(Bytes::from(text));
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
    pub fn set<H>(&mut self, headers: H) -> &mut Self
    where
        H: SetIntoHeaders,
    {
        if let Err(e) = headers.into_headers(self.inner.headers_mut()) {
            tracing::error!("failed to set headers: {}", e);
        }
        self
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
    pub fn append<K, V>(&mut self, key: K, values: V) -> &mut Self
    where
        K: TryInto<HeaderName, Error = InvalidHeaderName>,
        V: AppendIntoHeaderValues,
        Error: From<K::Error>,
    {
        let key = match key.try_into() {
            Ok(k) => k,
            Err(e) => {
                tracing::error!("failed to convert header name: {}", e);
                return self;
            }
        };
        if let Err(e) = values.append_to_header(self.inner.headers_mut(), key) {
            tracing::error!("failed to append header value: {}", e);
        }
        self
    }

    /// Send a *non-streaming* body.
    #[inline]
    pub fn send(&mut self, body: impl Into<Bytes>) -> &mut Self {
        *self.inner.body_mut() = HttpBody::full(body.into());
        self
    }

    /// Send a *streaming* body.
    #[inline]
    pub fn stream<S>(&mut self, stream: S) -> &mut Self
    where
        S: Stream<Item = Result<Bytes, BoxError>> + Send + Sync + 'static,
    {
        *self.inner.body_mut() = HttpBody::stream(stream);
        self
    }

    /// Send a *streaming* body with frames.
    #[inline]
    pub fn stream_frames<S>(&mut self, stream: S) -> &mut Self
    where
        S: Stream<Item = Result<Frame<Bytes>, BoxError>> + Send + Sync + 'static,
    {
        *self.inner.body_mut() = HttpBody::stream_frames(stream);
        self
    }

    #[inline]
    pub fn content_type<V>(&mut self, value: V) -> &mut Self
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
            .content_type("text/html; charset=utf-8");
        self
    }

    #[inline]
    pub fn json(&mut self, value: impl serde::Serialize) -> &mut Self {
        match serde_json::to_string(&value) {
            Ok(json_str) => self
                .status(StatusCode::OK)
                .send(json_str)
                .content_type("application/json; charset=utf-8"),
            Err(e) => {
                tracing::error!("failed to serialize JSON response: {}", e);
                self.status(StatusCode::INTERNAL_SERVER_ERROR)
                    .send("Internal Server Error")
                    .content_type("text/plain; charset=utf-8")
            }
        }
    }

    #[cfg(feature = "minijinja")]
    #[inline]
    pub fn get_render_ctx(&self) -> minijinja::Value {
        let mut ctx = minijinja::__context::make();
        self.app.with_locals(|l| {
            for (key, value) in l {
                ctx.insert(key.into(), minijinja::Value::from_serialize(value));
            }
        });
        for (key, value) in &self.locals {
            ctx.insert(key.into(), minijinja::Value::from_serialize(value));
        }
        minijinja::__context::build(ctx)
    }

    #[cfg(feature = "minijinja")]
    #[inline]
    fn get_rendered_template(&self, template: &str, c: minijinja::Value) -> Result<String, Error> {
        let template = self.app.render_env.get_template(template).map_err(|e| {
            tracing::warn!("template not found: {}", template);
            Error::from(e)
        })?;

        let rendered = template.render(&c).map_err(|e| {
            tracing::warn!("failed to render template {}: {}", template.name(), e);
            Error::from(e)
        })?;

        Ok(rendered)
    }

    #[cfg(feature = "minijinja")]
    fn render_template(&mut self, template: &str, c: minijinja::Value) -> &mut Self {
        let Ok(rendered) = self.get_rendered_template(template, c) else {
            return self.send_status(StatusCode::INTERNAL_SERVER_ERROR);
        };

        self.status(StatusCode::OK)
            .send(rendered)
            .content_type("text/html; charset=utf-8");

        self
    }

    #[cfg(feature = "minijinja")]
    #[inline]
    pub fn render(&mut self, template: &str) -> &mut Self {
        let ctx = self.get_render_ctx();
        self.render_template(template, ctx)
    }

    #[cfg(feature = "minijinja")]
    #[inline]
    pub fn render_with(&mut self, template: &str, value: minijinja::Value) -> &mut Self {
        let final_ctx = minijinja::context! {
            ..self.get_render_ctx(),
            ..value,
        };
        self.render_template(template, final_ctx)
    }

    /// Redirects to the specified location with an optional status code.
    /// If no status is provided, defaults to 302 Found.
    pub fn redirect(&mut self, location: impl AsRef<str>, status: Option<StatusCode>) -> &mut Self {
        // Set the Location header
        self.set((header::LOCATION, location.as_ref()));

        // Set status code (default to 302 Found)
        let status_code = status.unwrap_or(StatusCode::FOUND);
        self.status(status_code);

        self
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
