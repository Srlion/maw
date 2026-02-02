use std::{
    marker::PhantomData,
    time::{Duration, UNIX_EPOCH},
};

use http::StatusCode;
use httpdate::{fmt_http_date, parse_http_date};
use rust_embed::RustEmbed;

use crate::{async_fn::AsyncFn1, ctx::Ctx};

pub struct StaticFiles<E> {
    _marker: PhantomData<E>,
    prefix: &'static str,
    index: &'static str,
    cache_control: Option<String>,
}

impl<E: RustEmbed> StaticFiles<E> {
    pub fn new(prefix: &'static str) -> Self {
        Self {
            _marker: PhantomData,
            prefix: prefix.trim_matches('/'),
            index: "index.html",
            cache_control: None,
        }
    }

    pub fn index(mut self, file: &'static str) -> Self {
        self.index = file;
        self
    }

    pub fn max_age(mut self, seconds: u32) -> Self {
        self.cache_control = if seconds > 0 {
            Some(format!("max-age={seconds}"))
        } else {
            None
        };
        self
    }
}

impl<E: RustEmbed + Sync> AsyncFn1<&mut Ctx> for StaticFiles<E> {
    type Output = ();

    async fn call(&self, c: &mut Ctx) -> Self::Output {
        let uri_path = c.req.uri().path().trim_start_matches('/');

        let path = if self.prefix.is_empty() {
            uri_path
        } else if let Some(rest) = uri_path.strip_prefix(self.prefix) {
            rest.trim_start_matches('/')
        } else {
            c.res.send_status(StatusCode::NOT_FOUND);
            return;
        };

        let file = if path.is_empty() || path.ends_with('/') {
            E::get(&[path, self.index].concat())
        } else {
            E::get(path).or_else(|| E::get(&[path, self.index].concat()))
        };

        let Some(file) = file else {
            c.res.send_status(StatusCode::NOT_FOUND);
            return;
        };

        if let Some(last_modified) = file.metadata.last_modified() {
            let modified = UNIX_EPOCH + Duration::from_secs(last_modified);
            c.res.set(("Last-Modified", fmt_http_date(modified)));

            if let Some(ims) = c.req.headers().get("If-Modified-Since") {
                if let Ok(ims_str) = ims.to_str() {
                    if let Ok(ims_time) = parse_http_date(ims_str) {
                        if modified <= ims_time {
                            c.res.send_status(StatusCode::NOT_MODIFIED);
                            return;
                        }
                    }
                }
            }
        }

        let mime = mime_guess::from_path(path).first_or_octet_stream();
        c.res.set(("Content-Type", mime.as_ref()));

        if let Some(cc) = &self.cache_control {
            c.res.set(("Cache-Control", cc.as_str()));
        }

        c.res.send(file.data.into_owned());
    }
}
