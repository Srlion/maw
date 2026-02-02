use std::collections::HashMap;

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use smol_str::SmolStr;

use crate::{
    async_fn::AsyncFn1,
    ctx::Ctx,
    middlewares::cookie::{CookieOptions, CookieType},
    prelude::StatusError,
};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct SessionStore {
    data: HashMap<String, Vec<u8>>,
    #[serde(skip)]
    modified: bool,
}

impl SessionStore {
    /// Get a value from the session
    pub fn get<T: DeserializeOwned>(&self, key: impl AsRef<str>) -> Result<T, SessionError> {
        let bytes = self
            .data
            .get(key.as_ref())
            .ok_or_else(|| SessionError::NotFound(key.as_ref().into()))?;
        postcard::from_bytes(bytes).map_err(SessionError::from)
    }

    /// Set a value in the session
    pub fn set<T: Serialize>(&mut self, key: impl Into<String>, value: T) {
        let value = match postcard::to_stdvec(&value) {
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
    ///
    /// Returns true if the key was present
    pub fn remove(&mut self, key: &str) -> bool {
        self.modified = true;
        self.data.remove(key).is_some()
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
pub struct SessionMiddleware {
    /// Name of the session cookie
    ///
    /// Default: "maw.session"
    cookie_name: String,

    /// Cookie Type for the session cookie
    cookie_type: CookieType,

    /// Cookie options for the session cookie
    cookie_options: CookieOptions,
}

impl SessionMiddleware {
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
        assert!(
            !matches!(cookie_type, CookieType::Plain),
            "Session cookie type cannot be Plain for security reasons"
        );
        self.cookie_type = cookie_type;
        self
    }

    /// Set the cookie options for the session cookie
    pub fn cookie_options(mut self, options: CookieOptions) -> Self {
        self.cookie_options = options;
        self
    }
}

impl Default for SessionMiddleware {
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

impl AsyncFn1<&mut Ctx> for SessionMiddleware {
    type Output = ();

    async fn call(&self, c: &mut Ctx) -> Self::Output {
        // Load session from cookie
        {
            let session = c
                .cookies
                .get_typed::<SessionStore>(&self.cookie_name, &self.cookie_type)
                .unwrap_or_default();

            c.session = session;
        }

        c.next().await;

        // Save session to cookie if modified
        if c.session.is_modified() {
            let cookie_name = &self.cookie_name.clone();
            let cookie_options = self.cookie_options.clone();
            let session = std::mem::take(&mut c.session);
            c.cookies.set_typed(
                cookie_name,
                &session,
                &self.cookie_type,
                Some(cookie_options),
            );
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("Session key not found: {0}")]
    NotFound(SmolStr),

    #[error("Failed to deserialize session value")]
    Deserialize(#[from] postcard::Error),
}

impl From<SessionError> for StatusError {
    fn from(e: SessionError) -> Self {
        match e {
            SessionError::NotFound(key) => {
                StatusError::bad_request().brief(format!("Session key not found: {key}"))
            }
            SessionError::Deserialize(ref err) => {
                use postcard::Error::*;
                match err {
                    SerdeDeCustom => StatusError::unprocessable_entity()
                        .brief("Failed to deserialize session value into expected type"),
                    _ => StatusError::bad_request().brief("Invalid session data"),
                }
            }
        }
    }
}
