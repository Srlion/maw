use std::collections::HashMap;

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use smol_str::SmolStr;

mod cookie_storage;
mod storage;

use cookie_storage::CookieStorage;
pub use storage::SessionStorage;

use crate::{
    ctx::Ctx,
    handler::Handler,
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
pub struct SessionMiddleware<S: SessionStorage = CookieStorage> {
    storage: S,

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
        Self {
            storage: CookieStorage::default(),
            cookie_name: "maw.session".into(),
            cookie_type: CookieType::Signed,
            cookie_options: CookieOptions::new()
                .path("/")
                .http_only(true)
                .secure(true)
                .same_site(cookie::SameSite::Lax),
        }
    }
}

impl Default for SessionMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: SessionStorage> SessionMiddleware<S> {
    pub fn storage<T: SessionStorage>(self, storage: T) -> SessionMiddleware<T> {
        SessionMiddleware {
            storage,
            cookie_name: self.cookie_name,
            cookie_type: self.cookie_type,
            cookie_options: self.cookie_options,
        }
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
    pub fn cookie_type(mut self, t: CookieType) -> Self {
        assert!(
            !matches!(t, CookieType::Plain),
            "Session cookie cannot be Plain"
        );
        self.cookie_type = t;
        self
    }

    /// Set the cookie options for the session cookie
    pub fn cookie_options(mut self, opts: CookieOptions) -> Self {
        self.cookie_options = opts;
        self
    }
}

impl<S: SessionStorage> Handler<&mut Ctx> for SessionMiddleware<S> {
    type Output = ();

    async fn call(&self, c: &mut Ctx) {
        if S::INLINE {
            c.session = c
                .cookies
                .get_typed(&self.cookie_name, &self.cookie_type)
                .unwrap_or_default();

            c.next().await;

            if c.session.is_modified() {
                let session = std::mem::take(&mut c.session);
                c.cookies.set_typed(
                    &self.cookie_name,
                    &session,
                    &self.cookie_type,
                    Some(self.cookie_options.clone()),
                );
            }
        } else {
            let sid: Option<String> = c
                .cookies
                .get_typed(&self.cookie_name, &self.cookie_type)
                .ok();

            c.session = match &sid {
                Some(id) => self.storage.load(id).await.unwrap_or_default(),
                None => SessionStore::default(),
            };

            c.next().await;

            if c.session.is_modified() {
                let session = std::mem::take(&mut c.session);
                let id = sid.unwrap_or_else(|| self.storage.generate_id());
                self.storage.save(&id, &session).await;
                c.cookies.set_typed(
                    &self.cookie_name,
                    &id,
                    &self.cookie_type,
                    Some(self.cookie_options.clone()),
                );
            }
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
