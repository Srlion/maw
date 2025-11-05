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
}

impl Config {
    pub const DEFAULT: Self = Self {
        body_limit: 4 * 1024 * 1024,
        proxy_header: String::new(),
    };

    /// Create a new Config with default values
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::DEFAULT
    }
}
