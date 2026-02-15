use std::{
    marker::PhantomData,
    time::{Duration, UNIX_EPOCH},
};

use http::StatusCode;
use httpdate::{fmt_http_date, parse_http_date};
use rust_embed::RustEmbed;

use crate::{ctx::Ctx, handler::Handler};

pub struct StaticFiles<E> {
    _marker: PhantomData<E>,
    pub(crate) index: &'static str,
    cache_control: Option<String>,
    fallback_to: Option<&'static str>,
}

impl<E: RustEmbed> StaticFiles<E> {
    pub fn new(_: E) -> Self {
        Self {
            _marker: PhantomData,
            index: "index.html",
            cache_control: None,
            fallback_to: None,
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

    /// Serve a specific embedded file when no matching path is found,
    /// going through the same last-modified/cache-control logic as normal files.
    ///
    /// ```rust
    /// // SPA: let the client-side router handle all unknown paths
    /// StaticFiles::new(Assets).fallback_to("index.html")
    /// ```
    pub fn fallback_to(mut self, file: &'static str) -> Self {
        self.fallback_to = Some(file);
        self
    }
}

impl<E> Clone for StaticFiles<E> {
    fn clone(&self) -> Self {
        Self {
            _marker: PhantomData,
            index: self.index,
            cache_control: self.cache_control.clone(),
            fallback_to: self.fallback_to,
        }
    }
}

impl<E: RustEmbed + Sync> Handler<&mut Ctx> for StaticFiles<E> {
    type Output = ();

    async fn call(&self, c: &mut Ctx) -> Self::Output {
        let path = c.req.param_str("_");

        let (file, mime_path) = if path.is_empty() || path.ends_with('/') {
            let full = [path, self.index].concat();
            (E::get(&full), full)
        } else {
            match E::get(path) {
                Some(f) => (Some(f), path.to_string()),
                None => {
                    let full = [path, "/", self.index].concat();
                    (E::get(&full), full)
                }
            }
        };

        let (file, mime_path) = match file {
            Some(f) => (f, mime_path),
            None => match self.fallback_to.and_then(|p| E::get(p).map(|f| (f, p))) {
                Some((f, p)) => (f, p.to_string()),
                None => {
                    c.res.send_status(StatusCode::NOT_FOUND);
                    return;
                }
            },
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

        let mime = mime_guess::from_path(&mime_path).first_or_octet_stream();
        c.res.set(("Content-Type", mime.as_ref()));

        if let Some(cc) = &self.cache_control {
            c.res.set(("Cache-Control", cc.as_str()));
        }

        c.res.send(file.data.into_owned());
    }
}
