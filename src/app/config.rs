#[cfg(feature = "session")]
use crate::session::SessionConfig;

#[derive(Clone, Debug)]
pub struct Config {
    /// Max body size that the server accepts.
    ///
    /// Default: 4MB
    pub(crate) body_limit: usize,

    /// ProxyHeader will enable c.req.ip() to return the value of the given header key
    /// By default c.req.ip() will return the Remote IP from the TCP connection
    /// This property can be useful if you are behind a load balancer: X-Forwarded-*
    /// NOTE: headers are easily spoofed and the detected IP addresses are unreliable.
    ///
    /// Default: ""
    pub(crate) proxy_header: String,

    #[cfg(feature = "cookie")]
    pub(crate) cookie_key: cookie::Key,

    #[cfg(feature = "session")]
    pub(crate) session: SessionConfig,
}

impl Config {
    /// Create a new Config with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum body size that the server accepts
    ///
    /// Default: 4MB
    pub fn body_limit(mut self, limit: usize) -> Self {
        self.body_limit = limit;
        self
    }

    /// ProxyHeader will enable c.req.ip() to return the value of the given header key
    /// By default c.req.ip() will return the Remote IP from the TCP connection
    /// This property can be useful if you are behind a load balancer: X-Forwarded-*
    /// NOTE: headers are easily spoofed and the detected IP addresses are unreliable.
    ///
    /// Default: ""
    pub fn proxy_header(mut self, header: impl Into<String>) -> Self {
        self.proxy_header = header.into();
        self
    }

    /// Set the cookie key used for signing/encrypting cookies
    /// This is required if you are using signed or encrypted cookies
    #[cfg(feature = "cookie")]
    pub fn cookie_key(mut self, key: impl Into<Vec<u8>>) -> Self {
        self.cookie_key = cookie::Key::from(&key.into());
        self
    }

    /// Set the session configuration
    #[cfg(feature = "session")]
    pub fn session(mut self, config: SessionConfig) -> Self {
        self.session = config;
        self
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            body_limit: 4 * 1024 * 1024,
            proxy_header: String::new(),
            #[cfg(feature = "cookie")]
            cookie_key: cookie::Key::generate(),
            #[cfg(feature = "session")]
            session: SessionConfig::default(),
        }
    }
}
