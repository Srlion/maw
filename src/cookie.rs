pub use cookie::{Cookie, CookieJar, Key, SameSite};
use serde::{Serialize, de::DeserializeOwned};

use crate::{ctx::Ctx, error::Error};

#[derive(Clone, Copy, Debug)]
pub enum CookieType {
    Plain,
    Signed,
    Encrypted,
}

impl Ctx {
    fn cookie_key(&self) -> &cookie::Key {
        &self.app().config.cookie_key
    }

    pub fn get_cookie<T: DeserializeOwned>(
        &self,
        name: &str,
        cookie_type: CookieType,
    ) -> Result<Option<T>, Error> {
        let value = match cookie_type {
            CookieType::Plain => self.cookies.get(name).map(|c| c.value().to_owned()),
            CookieType::Signed => self
                .cookies
                .signed(self.cookie_key())
                .get(name)
                .map(|c| c.value().to_owned()),
            CookieType::Encrypted => self
                .cookies
                .private(self.cookie_key())
                .get(name)
                .map(|c| c.value().to_owned()),
        };

        value
            .map(|v| serde_json::from_str(&v))
            .transpose()
            .map_err(Error::from)
    }

    pub fn set_cookie<T: Serialize>(
        &mut self,
        name: &str,
        value: &T,
        cookie_type: CookieType,
        options: Option<CookieOptions>,
    ) {
        let value_str = match serde_json::to_string(value) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("failed to serialize cookie value: {}", e);
                return;
            }
        };
        let cookie = if let Some(opts) = options {
            self.build_cookie(name, &value_str, opts)
        } else {
            Cookie::new(name.to_owned(), value_str)
        };

        match cookie_type {
            CookieType::Plain => self.cookies.add(cookie),
            CookieType::Signed => self
                .cookies
                .signed_mut(&self.cookie_key().clone())
                .add(cookie),
            CookieType::Encrypted => self
                .cookies
                .private_mut(&self.cookie_key().clone())
                .add(cookie),
        }
    }

    pub fn clear_cookie(&mut self, name: &str) {
        self.cookies.remove(Cookie::from(name.to_owned()));
    }

    fn build_cookie<'a>(&self, name: &str, value: &str, options: CookieOptions) -> Cookie<'a> {
        let mut builder = Cookie::build((name.to_owned(), value.to_owned()));

        builder = builder.path(options.path.unwrap_or("/".to_string()));

        if let Some(domain) = options.domain {
            builder = builder.domain(domain);
        }
        if let Some(secure) = options.secure {
            builder = builder.secure(secure);
        }
        if let Some(http_only) = options.http_only {
            builder = builder.http_only(http_only);
        }
        if let Some(same_site) = options.same_site {
            builder = builder.same_site(same_site);
        }
        if let Some(max_age) = options.max_age {
            builder = builder.max_age(max_age);
        }
        if let Some(expires) = options.expires {
            builder = builder.expires(expires);
        }

        builder.build()
    }
}

pub async fn middleware(c: &mut Ctx) {
    // Initialize cookies from request
    {
        let mut jar = CookieJar::new();
        if let Some(cookie_header) = c.req.get(http::header::COOKIE) {
            for cookie_str in cookie_header.split(';') {
                if let Ok(cookie) = Cookie::parse(cookie_str.trim().to_owned()) {
                    jar.add_original(cookie);
                }
            }
        }
        c.cookies = jar;
    }

    c.next().await;

    // Set cookies in response, if any
    for cookie in c.cookies.delta() {
        if let Ok(header_value) = http::HeaderValue::from_str(&cookie.to_string()) {
            c.res
                .headers_mut()
                .append(http::header::SET_COOKIE, header_value);
        }
    }
}

/// Options for setting cookies
#[derive(Default, Clone, Debug)]
pub struct CookieOptions {
    path: Option<String>,
    domain: Option<String>,
    secure: Option<bool>,
    http_only: Option<bool>,
    same_site: Option<SameSite>,
    max_age: Option<cookie::time::Duration>,
    expires: Option<cookie::time::OffsetDateTime>,
}

impl CookieOptions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the path for the cookie
    ///
    /// Default: "/"
    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    pub fn domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = Some(secure);
        self
    }

    pub fn http_only(mut self, http_only: bool) -> Self {
        self.http_only = Some(http_only);
        self
    }

    pub fn same_site(mut self, same_site: SameSite) -> Self {
        self.same_site = Some(same_site);
        self
    }

    pub fn max_age(mut self, max_age: cookie::time::Duration) -> Self {
        self.max_age = Some(max_age);
        self
    }

    pub fn expires(mut self, expires: cookie::time::OffsetDateTime) -> Self {
        self.expires = Some(expires);
        self
    }
}
