use http::{Method, StatusCode};

use crate::{
    ctx::Ctx,
    middlewares::cookie::{CookieOptions, CookieType},
};

const CSRF_HEADER: &str = "X-CSRF-Token";

#[derive(Clone, Copy, Debug)]
pub enum CsrfStorage {
    Cookie,
    #[cfg(feature = "session")]
    Session,
}

#[derive(Clone, Debug)]
pub struct CsrfConfig {
    storage: CsrfStorage,
    key_name: String,
    safe_methods: Vec<Method>,
    cookie_type: CookieType,
    cookie_options: CookieOptions,
}

impl Default for CsrfConfig {
    fn default() -> Self {
        Self {
            storage: CsrfStorage::Cookie,
            key_name: "csrf_token".to_string(),
            safe_methods: vec![Method::GET, Method::HEAD, Method::OPTIONS, Method::TRACE],
            cookie_type: CookieType::Signed,
            cookie_options: CookieOptions::new()
                .path("/")
                .http_only(true)
                .same_site(crate::middlewares::cookie::SameSite::Strict),
        }
    }
}

impl CsrfConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn storage(mut self, storage: CsrfStorage) -> Self {
        self.storage = storage;
        self
    }

    pub fn key_name(mut self, name: impl Into<String>) -> Self {
        self.key_name = name.into();
        self
    }

    pub fn safe_methods(mut self, methods: Vec<Method>) -> Self {
        self.safe_methods = methods;
        self
    }

    pub fn cookie_type(mut self, cookie_type: CookieType) -> Self {
        self.cookie_type = cookie_type;
        self
    }

    pub fn cookie_options(mut self, options: CookieOptions) -> Self {
        self.cookie_options = options;
        self
    }
}

fn generate_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let mut result = String::with_capacity(64);
    for byte in bytes {
        result.push_str(&format!("{:02x}", byte));
    }
    result
}

impl Ctx {
    /// Get the CSRF token for this request
    pub fn csrf_token(&self) -> &str {
        self.req
            .locals
            .get::<&str>("csrf_token")
            .map(|s| *s)
            .unwrap_or("")
    }

    /// Get the name of the CSRF header
    pub fn csrf_header() -> &'static str {
        CSRF_HEADER
    }
}

pub fn middleware(
    config: CsrfConfig,
) -> impl Fn(&mut Ctx) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> + Clone
{
    move |c: &mut Ctx| {
        let config = config.clone();
        Box::pin(async move {
            c.res.locals.insert("csrf_header", CSRF_HEADER);

            let is_safe = config.safe_methods.contains(c.req.method());

            let token = match config.storage {
                CsrfStorage::Cookie => c
                    .cookies
                    .get_typed::<String>(&config.key_name, &config.cookie_type)
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| {
                        let token = generate_token();
                        c.cookies.set_typed(
                            &config.key_name,
                            &token,
                            &config.cookie_type,
                            Some(config.cookie_options.clone()),
                        );
                        token
                    }),
                #[cfg(feature = "session")]
                CsrfStorage::Session => {
                    c.session
                        .get::<String>(&config.key_name)
                        .unwrap_or_else(|| {
                            let token = generate_token();
                            c.session.set(&config.key_name, token.clone());
                            token
                        })
                }
            };

            if !is_safe {
                let submitted_token = c.req.get(CSRF_HEADER);
                let is_valid = submitted_token
                    .as_ref()
                    .map(|submitted| {
                        constant_time_eq::constant_time_eq(submitted.as_bytes(), token.as_bytes())
                    })
                    .unwrap_or(false);

                if !is_valid {
                    c.res.send_status(StatusCode::FORBIDDEN);
                    return;
                }
            }

            c.req.locals.insert("csrf_token", token);

            c.next().await;
        })
    }
}
