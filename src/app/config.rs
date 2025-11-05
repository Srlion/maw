#[cfg(feature = "session")]
use crate::cookie::{CookieOptions, CookieType};

#[derive(Clone, Debug)]
pub struct Config {
    /// Max body size that the server accepts.
    ///
    /// Default: 4MB
    pub body_limit: usize,

    /// ProxyHeader will enable c.req.ip() to return the value of the given header key
    /// By default c.req.ip() will return the Remote IP from the TCP connection
    /// This property can be useful if you are behind a load balancer: X-Forwarded-*
    /// NOTE: headers are easily spoofed and the detected IP addresses are unreliable.
    ///
    /// Default: ""
    pub proxy_header: String,

    #[cfg(feature = "cookie")]
    pub cookie_key: Option<cookie::Key>,

    #[cfg(feature = "session")]
    pub session: SessionConfig,
}

#[cfg(feature = "session")]
#[derive(Clone, Debug)]
pub struct SessionConfig {
    /// Name of the session cookie
    ///
    /// Default: "maw.session"
    pub cookie_name: String,

    /// Cookie Type for the session cookie
    pub cookie_type: CookieType,

    /// Cookie options for the session cookie
    pub cookie_options: CookieOptions,
}

#[cfg(feature = "session")]
impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            cookie_name: "maw.session".to_string(),
            cookie_type: CookieType::Signed,
            cookie_options: CookieOptions {
                path: Some("/".to_string()),
                http_only: Some(true),
                secure: Some(true),
                same_site: Some(cookie::SameSite::Lax),
                ..Default::default()
            },
        }
    }
}

impl Config {
    /// Create a new Config with default values
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            body_limit: 4 * 1024 * 1024,
            proxy_header: String::new(),
            #[cfg(feature = "cookie")]
            cookie_key: None,
            #[cfg(feature = "session")]
            session: SessionConfig::default(),
        }
    }
}
