use std::collections::HashMap;

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::{
    async_fn::AsyncFn1,
    cookie::{CookieOptions, CookieType},
    ctx::Ctx,
};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct SessionStore {
    data: HashMap<String, Value>,
    #[serde(skip)]
    modified: bool,
}

impl SessionStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a value from the session
    pub fn get<T: DeserializeOwned>(&self, key: impl AsRef<str>) -> Option<T> {
        self.data
            .get(key.as_ref())
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Set a value in the session
    pub fn set<T: Serialize>(&mut self, key: impl Into<String>, value: T) {
        let value = match serde_json::to_value(value) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("failed to serialize session value: {}", e);
                return;
            }
        };
        self.data.insert(key.into(), value);
        self.modified = true;
    }

    /// Remove a value from the session
    pub fn remove(&mut self, key: &str) -> Option<Value> {
        self.modified = true;
        self.data.remove(key)
    }

    /// Clear all session data
    pub fn clear(&mut self) {
        self.data.clear();
        self.modified = true;
    }

    /// Check if the session has been modified
    pub fn is_modified(&self) -> bool {
        self.modified
    }

    /// Check if a key exists in the session
    pub fn contains(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }

    /// Get all keys in the session
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.data.keys()
    }
}

/// Has to be used after cookie middleware
#[derive(Clone, Debug)]
pub struct Session {
    /// Name of the session cookie
    ///
    /// Default: "maw.session"
    cookie_name: String,

    /// Cookie Type for the session cookie
    cookie_type: CookieType,

    /// Cookie options for the session cookie
    cookie_options: CookieOptions,
}

impl Session {
    /// Create a new SessionConfig with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the name of the session cookie
    ///
    /// Default: "maw.session"
    pub fn cookie_name(mut self, name: impl Into<String>) -> Self {
        self.cookie_name = name.into();
        self
    }

    /// Set the cookie type for the session cookie
    ///
    /// Default: CookieType::Signed
    pub fn cookie_type(mut self, cookie_type: CookieType) -> Self {
        self.cookie_type = cookie_type;
        self
    }

    /// Set the cookie options for the session cookie
    pub fn cookie_options(mut self, options: CookieOptions) -> Self {
        self.cookie_options = options;
        self
    }
}

impl Default for Session {
    fn default() -> Self {
        Self {
            cookie_name: "maw.session".to_string(),
            cookie_type: CookieType::Signed,
            cookie_options: CookieOptions::new()
                .path("/")
                .http_only(true)
                .secure(true)
                .same_site(cookie::SameSite::Lax),
        }
    }
}

impl AsyncFn1<&mut Ctx> for Session {
    type Output = ();

    async fn call(&self, c: &mut Ctx) -> Self::Output {
        // Load session from cookie
        {
            let session = c
                .get_cookie::<SessionStore>(&self.cookie_name, self.cookie_type)
                .ok()
                .flatten()
                .unwrap_or_default();

            c.session = session;
        }

        c.next().await;

        // Save session to cookie if modified
        if c.session.is_modified() {
            let cookie_name = &self.cookie_name.clone();
            let cookie_type = self.cookie_type;
            let cookie_options = self.cookie_options.clone();
            let session = std::mem::take(&mut c.session);
            c.set_cookie(cookie_name, &session, cookie_type, Some(cookie_options));
        }
    }
}
