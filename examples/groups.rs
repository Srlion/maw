use maw::{LoggingMiddleware, prelude::*};

#[tokio::main]
async fn main() -> Result<(), MawError> {
    tracing_subscriber::fmt::init();

    let api = Router::group("/api")
        .get("/users", async |c: &mut Ctx| c.res.json(&["Alice", "Bob"]))
        .post("/users", async |c: &mut Ctx| {
            c.res.send("Created");
        });

    let admin = Router::group("/admin")
        .get("/stats", async |c: &mut Ctx| {
            c.res.send("Stats");
        })
        .get("/test", async |c: &mut Ctx| {
            c.res.send("Admin Test");
        });

    let test = Router::group("/test")
        .get("/ping", async |c: &mut Ctx| {
            c.res.send("pong");
        })
        .get("/hello", async |c: &mut Ctx| {
            c.res.send("Hello, World!");
        })
        .push(admin.clone()); // Router is Arc<Mutex<RouterInner>>, so it's the same instance

    let app = App::new()
        //
        .router(
            Router::new()
                .middleware(LoggingMiddleware::new())
                .push(api)
                .push(admin)
                .push(test),
        );

    app.listen("127.0.0.1:3000").await
}
