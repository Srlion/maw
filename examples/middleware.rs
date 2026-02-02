use std::sync::Arc;

use maw::prelude::*;

async fn auth(c: &mut Ctx) {
    if c.req.get("authorization").is_none() {
        c.res.send_status(StatusCode::UNAUTHORIZED);
    } else {
        c.next().await;
    }
}

#[tokio::main]
async fn main() -> Result<(), MawError> {
    tracing_subscriber::fmt::init();

    let upvalue = Arc::new(42);

    let app = App::new().router(
        Router::new()
            .get("/public", async |_: &mut Ctx| "Public route")
            // Unfortunately, async closures cannot use captured variables in this context, so we use `WithState` as a workaround.
            .get(
                "/state",
                WithState(upvalue, async |_: &mut Ctx, state: Arc<i32>| {
                    format!("State value is: {state}")
                }),
            )
            .get(
                "/protected",
                (auth, async |c: &mut Ctx| {
                    c.res.send("Protected data");
                }),
            )
            .get(
                "/protected2",
                (
                    async |c: &mut Ctx| {
                        if c.req.get("authorization").is_none() {
                            c.res.send_status(StatusCode::UNAUTHORIZED);
                        } else {
                            c.next().await;
                        }
                    },
                    async |c: &mut Ctx| {
                        c.res.send("Protected 2 data");
                    },
                ),
            ),
    );

    app.listen("127.0.0.1:3000").await
}
