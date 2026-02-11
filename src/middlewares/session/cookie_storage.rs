use crate::ctx::Ctx;

use super::{SessionStorage, SessionStore};

#[derive(Clone, Copy, Debug, Default)]
pub struct CookieStorage;

impl SessionStorage for CookieStorage {
    const INLINE: bool = true;

    async fn load(&self, _: &mut Ctx, _: &str) -> Option<SessionStore> {
        None
    }

    async fn save(&self, _: &mut Ctx, _: &str, _: &SessionStore) {}
}
