use maw::middlewares::cookie::{self, CookieOptions};
use maw::prelude::*;
use maw::{CookieMiddleware, SessionMiddleware};

#[tokio::main]
async fn main() -> Result<(), MawError> {
    tracing_subscriber::fmt::init();

    let app = App::new().router(
        Router::new()
            .middleware((
                CookieMiddleware::new()
                    .key("your-secret-key-min-64-bytes!!your-secret-key-min-64-bytes!!!!!!"),
                SessionMiddleware::new()
                    .cookie_name("app.session")
                    .cookie_options(
                        CookieOptions::new()
                            .secure(is_prod())
                            .http_only(true)
                            .same_site(cookie::SameSite::Strict)
                            .path("/"),
                    ),
            ))
            .get("/set", async |c: &mut Ctx| {
                c.cookies.set("user", &"Alice", None);
                c.session.set("visits", 1u32);
                c.res.send("Set!");
            })
            .get("/get", async |c: &mut Ctx| {
                let user: String = c.cookies.get("user")?;
                let visits: u32 = c.session.get("visits")?;
                c.res.send(format!("User: {}, Visits: {}", user, visits));
                c.session.set("visits", visits + 1);
                Ok(())
            }),
    );

    app.listen("127.0.0.1:3000").await
}

fn is_prod() -> bool {
    std::env::var("APP_ENV")
        .unwrap_or_default()
        .starts_with("prod")
}
