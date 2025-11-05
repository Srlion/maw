use cookie::{Cookie, CookieJar, SameSite};
use serde::{Serialize, de::DeserializeOwned};

use crate::{ctx::Ctx, error::Error};

#[derive(Clone, Copy, Debug)]
pub enum CookieType {
    Plain,
    Signed,
    Encrypted,
}

/// Options for setting cookies
#[derive(Default, Clone, Debug)]
pub struct CookieOptions {
    pub path: Option<String>,
    pub domain: Option<String>,
    pub secure: Option<bool>,
    pub http_only: Option<bool>,
    pub same_site: Option<SameSite>,
    pub max_age: Option<cookie::time::Duration>,
    pub expires: Option<cookie::time::OffsetDateTime>,
}

impl Ctx {
    fn cookie_key(&self) -> &cookie::Key {
        self.app()
            .config
            .cookie_key
            .as_ref()
            .expect("Cookie key not set")
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
    ) -> Result<(), Error> {
        let value_str = serde_json::to_string(value)?;
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

        Ok(())
    }

    pub fn clear_cookie(&mut self, name: &str) {
        self.cookies.remove(Cookie::from(name.to_owned()));
    }

    fn build_cookie<'a>(&self, name: &str, value: &str, options: CookieOptions) -> Cookie<'a> {
        let mut builder = Cookie::build((name.to_owned(), value.to_owned()));

        if let Some(path) = options.path {
            builder = builder.path(path);
        }
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
