use super::{SessionStorage, SessionStore};

#[derive(Clone, Copy, Debug, Default)]
pub struct CookieStorage;

impl SessionStorage for CookieStorage {
    const INLINE: bool = true;

    async fn load(&self, _: &str) -> Option<SessionStore> {
        None
    }

    async fn save(&self, _: &str, _: &SessionStore) {}
}
