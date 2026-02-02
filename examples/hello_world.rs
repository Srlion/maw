use maw::prelude::*;

#[tokio::main]
async fn main() -> Result<(), MawError> {
    tracing_subscriber::fmt::init();

    let app = App::new().router(
        Router::new()
            .get("/", async |_: &mut Ctx| "Hello World!")
            // Uses the awesome https://crates.io/crates/matchit crate for path params
            .get("/user/{id}", async |c: &mut Ctx| {
                let id: u32 = c.req.param("id")?;
                c.res.json(format!("User {id}"));
                Ok(())
            })
            .post("/data", async |c: &mut Ctx| {
                #[derive(serde::Deserialize)]
                struct Data {
                    name: String,
                    age: u8,
                }

                let data = c.req.parse_body::<Data>(None).await?;
                c.res.json(format!("Got: {}, {}", data.name, data.age));

                Ok(())
            }),
    );

    app.listen("127.0.0.1:3000").await
}
