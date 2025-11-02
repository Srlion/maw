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

    let router = Router::new()
        .get(async |ctx: &mut Ctx| {
            ctx.res.send("Hello, world!");
            Ok(())
        });

    app.router(router).listen("127.0.0.1:3000").await?;
    Ok(())
}
```

## License

MIT
