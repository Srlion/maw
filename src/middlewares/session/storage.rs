use super::SessionStore;

pub trait SessionStorage: Send + Sync {
    /// If true, entire session stored in cookie (no external storage)
    const INLINE: bool = false;

    fn load(&self, id: &str) -> impl std::future::Future<Output = Option<SessionStore>> + Send;
    fn save(
        &self,
        id: &str,
        session: &SessionStore,
    ) -> impl std::future::Future<Output = ()> + Send;

    fn generate_id(&self) -> String {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        URL_SAFE_NO_PAD.encode(rand::random::<[u8; 18]>())
    }
}
