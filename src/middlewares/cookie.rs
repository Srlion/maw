pub use cookie::{Cookie, CookieJar, Key, SameSite};
use serde::{Serialize, de::DeserializeOwned};

use crate::{async_fn::AsyncFn1, ctx::Ctx, error::Error};

#[derive(Clone, Debug)]
pub enum CookieType {
    Plain,
    Signed,
    Encrypted,
}

#[derive(Clone, Debug, Default)]
pub struct CookieStore {
    jar: CookieJar,
    key: Option<cookie::Key>,
}

impl CookieStore {
    pub fn key(&self) -> &cookie::Key {
        self.key.as_ref().expect("Cookie key is not set")
    }

    pub fn remove(&mut self, name: impl Into<Cookie<'static>>) {
        self.jar.remove(name);
    }

    pub fn get<T: DeserializeOwned>(&self, name: &str) -> Result<Option<T>, Error> {
        let value = self.jar.get(name).map(|c| c.value().to_owned());
        value
            .map(|v| serde_json::from_str(&v))
            .transpose()
            .map_err(Error::from)
    }

    pub fn get_signed<T: DeserializeOwned>(&self, name: &str) -> Result<Option<T>, Error> {
        let value = self
            .jar
            .signed(self.key())
            .get(name)
            .map(|c| c.value().to_owned());
        value
            .map(|v| serde_json::from_str(&v))
            .transpose()
            .map_err(Error::from)
    }

    pub fn get_encrypted<T: DeserializeOwned>(&self, name: &str) -> Result<Option<T>, Error> {
        let value = self
            .jar
            .private(self.key())
            .get(name)
            .map(|c| c.value().to_owned());
        value
            .map(|v| serde_json::from_str(&v))
            .transpose()
            .map_err(Error::from)
    }

    pub fn get_typed<T: DeserializeOwned>(
        &self,
        name: &str,
        cookie_type: &CookieType,
    ) -> Result<Option<T>, Error> {
        match cookie_type {
            CookieType::Plain => self.get(name),
            CookieType::Signed => self.get_signed(name),
            CookieType::Encrypted => self.get_encrypted(name),
        }
    }

    pub fn set<T: Serialize>(&mut self, name: &str, value: &T, options: Option<CookieOptions>) {
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

        self.jar.add(cookie)
    }

    pub fn set_signed<T: Serialize>(
        &mut self,
        name: &str,
        value: &T,
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

        self.jar.signed_mut(&self.key().clone()).add(cookie)
    }

    pub fn set_encrypted<T: Serialize>(
        &mut self,
        name: &str,
        value: &T,
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

        self.jar.private_mut(&self.key().clone()).add(cookie)
    }

    pub fn set_typed<T: Serialize>(
        &mut self,
        name: &str,
        value: &T,
        cookie_type: &CookieType,
        options: Option<CookieOptions>,
    ) {
        match cookie_type {
            CookieType::Plain => self.set(name, value, options),
            CookieType::Signed => self.set_signed(name, value, options),
            CookieType::Encrypted => self.set_encrypted(name, value, options),
        }
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

pub struct CookieMiddleware {
    key: Option<cookie::Key>,
}

impl CookieMiddleware {
    pub fn new() -> Self {
        Self { key: None }
    }

    pub fn with_key(mut self, key: impl Into<CookieKey>) -> Self {
        self.key = Some(key.into().into_cookie_key());
        self
    }
}

impl AsyncFn1<&mut Ctx> for CookieMiddleware {
    type Output = ();

    async fn call(&self, c: &mut Ctx) -> Self::Output {
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
            c.cookies = CookieStore {
                jar,
                key: self.key.clone(),
            };
        }

        c.next().await;

        // Set cookies in response, if any
        for cookie in c.cookies.jar.delta() {
            if let Ok(header_value) = http::HeaderValue::from_str(&cookie.to_string()) {
                c.res
                    .headers_mut()
                    .append(http::header::SET_COOKIE, header_value);
            }
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

pub enum CookieKey {
    Key(cookie::Key),
    Bytes(Vec<u8>),
}

impl From<cookie::Key> for CookieKey {
    fn from(key: cookie::Key) -> Self {
        Self::Key(key)
    }
}

impl From<&cookie::Key> for CookieKey {
    fn from(key: &cookie::Key) -> Self {
        Self::Key(key.clone())
    }
}

impl From<Vec<u8>> for CookieKey {
    fn from(bytes: Vec<u8>) -> Self {
        Self::Bytes(bytes)
    }
}

impl From<&Vec<u8>> for CookieKey {
    fn from(bytes: &Vec<u8>) -> Self {
        Self::Bytes(bytes.clone())
    }
}

impl From<&[u8]> for CookieKey {
    fn from(bytes: &[u8]) -> Self {
        Self::Bytes(bytes.to_vec())
    }
}

impl From<String> for CookieKey {
    fn from(s: String) -> Self {
        Self::Bytes(s.into_bytes())
    }
}

impl From<&String> for CookieKey {
    fn from(s: &String) -> Self {
        Self::Bytes(s.as_bytes().to_vec())
    }
}

impl From<&str> for CookieKey {
    fn from(s: &str) -> Self {
        Self::Bytes(s.as_bytes().to_vec())
    }
}

impl CookieKey {
    fn into_cookie_key(self) -> cookie::Key {
        match self {
            Self::Key(k) => k,
            Self::Bytes(b) => cookie::Key::from(&b),
        }
    }
}
