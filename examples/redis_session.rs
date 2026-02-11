use maw::middlewares::cookie::{self, CookieOptions};
use maw::middlewares::session::{SessionStorage, SessionStore};
use maw::prelude::*;
use maw::{CookieMiddleware, SessionMiddleware};
use redis::AsyncCommands;

struct RedisStorage {
    client: redis::Client,
    ttl: u64,
}

impl RedisStorage {
    fn new(url: &str, ttl: u64) -> Self {
        Self {
            client: redis::Client::open(url).expect("invalid redis url"),
            ttl,
        }
    }
}

impl SessionStorage for RedisStorage {
    async fn load(&self, _: &mut Ctx, id: &str) -> Option<SessionStore> {
        let mut conn = self.client.get_multiplexed_async_connection().await.ok()?;
        let data: Vec<u8> = conn.get(["session:", id].concat()).await.ok()?;
        postcard::from_bytes(&data).ok()
    }

    async fn save(&self, _: &mut Ctx, id: &str, session: &SessionStore) {
        let Ok(mut conn) = self.client.get_multiplexed_async_connection().await else {
            return;
        };
        let Ok(data) = postcard::to_stdvec(session) else {
            return;
        };
        let _: Result<(), _> = conn.set_ex(["session:", id].concat(), data, self.ttl).await;
    }
}

#[tokio::main]
async fn main() -> Result<(), MawError> {
    tracing_subscriber::fmt::init();

    let app = App::new().router(
        Router::new()
            .middleware((
                CookieMiddleware::new()
                    .key("your-secret-key-min-64-bytes!!your-secret-key-min-64-bytes!!!!!!"),
                SessionMiddleware::new()
                    .storage(RedisStorage::new("redis://127.0.0.1", 3600))
                    .cookie_name("app.sid")
                    .cookie_options(
                        CookieOptions::new()
                            .secure(false)
                            .http_only(true)
                            .same_site(cookie::SameSite::Strict)
                            .path("/"),
                    ),
            ))
            .get("/set", async |c: &mut Ctx| {
                c.session.set("visits", 1u32);
                c.res.send("Session set!");
            })
            .get("/get", async |c: &mut Ctx| {
                let visits: u32 = c.session.get("visits").unwrap_or(0);
                c.session.set("visits", visits + 1);
                c.res.send(format!("Visits: {visits}"));
                Ok(())
            }),
    );

    app.listen("127.0.0.1:3000").await
}
