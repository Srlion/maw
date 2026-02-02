use http::StatusCode;
use rust_embed::RustEmbed;
use std::marker::PhantomData;

use crate::{async_fn::AsyncFn1, ctx::Ctx};

pub struct StaticFiles<E> {
    _marker: PhantomData<E>,
    prefix: &'static str,
    index: &'static str,
    max_age: u32,
}

impl<E: RustEmbed> StaticFiles<E> {
    pub fn new(prefix: &'static str) -> Self {
        Self {
            _marker: PhantomData,
            prefix: prefix.trim_matches('/'),
            index: "index.html",
            max_age: 0,
        }
    }

    pub fn index(mut self, file: &'static str) -> Self {
        self.index = file;
        self
    }

    pub fn max_age(mut self, seconds: u32) -> Self {
        self.max_age = seconds;
        self
    }
}

impl<E: RustEmbed + Send + Sync + 'static> AsyncFn1<&mut Ctx> for StaticFiles<E> {
    type Output = ();

    async fn call(&self, c: &mut Ctx) -> Self::Output {
        let uri_path = c.req.uri().path().trim_start_matches('/');

        // Strip prefix
        let path = if self.prefix.is_empty() {
            uri_path
        } else if let Some(rest) = uri_path.strip_prefix(self.prefix) {
            rest.trim_start_matches('/')
        } else {
            c.res.send_status(StatusCode::NOT_FOUND);
            return;
        };

        // Try exact path, then with index
        let file = if path.is_empty() || path.ends_with('/') {
            E::get(&format!("{}{}", path, self.index))
        } else {
            E::get(path).or_else(|| E::get(&format!("{}/{}", path, self.index)))
        };

        let Some(file) = file else {
            c.res.send_status(StatusCode::NOT_FOUND);
            return;
        };

        let mime = mime_guess::from_path(path).first_or_octet_stream();
        c.res.set(("Content-Type", mime.as_ref()));

        if self.max_age > 0 {
            c.res
                .set(("Cache-Control", format!("max-age={}", self.max_age)));
        }

        c.res.send(file.data.into_owned());
    }
}
