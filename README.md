# Maw

A minimal, fast web framework for Rust built on Hyper.

## Features

- **Simple routing** with path parameters
- **Middleware support** (global and local)
- **Template rendering** with MiniJinja
- **Request/response helpers** for JSON, forms, and XML
- **Type-safe locals** for sharing data
- **Graceful shutdown** built-in

## Quick Start

```rust
use maw::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();

    let router = Router::new().get("/", async |c: &mut Ctx| {
        c.res.send("Hello, world!");
        Ok(())
    });

    // to have it with middlewares, can be done like this:
    let router = Router::new()
        .middleware(async |c: &mut Ctx| {
            c.next().await;
        })
        .get("/", async |c: &mut Ctx| {
            c.res.send("Hello, world!");
            Ok(())
        });

    // or
    let router = Router::new().get(
        "/",
        (
            async |c: &mut Ctx| {
                c.next().await;
            },
            async |c: &mut Ctx| {
                c.res.send("Hello, world!");
                Ok(())
            },
        ),
    );

    app.router(router).listen("127.0.0.1:3000").await?;
    Ok(())
}
```

## License

MIT
