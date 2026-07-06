# Maw

A minimal web framework for Rust. No macros, just functions.

[![GitHub stars](https://img.shields.io/github/stars/srlion/maw?style=social)](https://github.com/srlion/maw)
[![Sponsor](https://img.shields.io/badge/sponsor-❤-ff69b4)](https://github.com/sponsors/srlion)

## Why

- No macros. Routes are just functions, nothing hidden behind attributes.
- `c.next().await` works like Express/Fiber middleware. Call it, then inspect or modify the response after.
- `println!("{router:#?}")` prints every route with its handlers and, in debug builds, source location.
- Handlers can be closures, plain functions, or structs.

## Quick Start

```rust
use maw::prelude::*;

#[tokio::main]
async fn main() -> Result<(), MawError> {
    let app = App::new().router(
        Router::new()
            .get("/", async |_: &mut Ctx| "Hello!")
            .get("/user/{id}", async |c: &mut Ctx| {
                let id: u32 = c.req.param("id")?;
                c.res.json(format!("User {id}"));
                Ok(())
            })
    );
    app.listen("127.0.0.1:3000").await
}
```

## Middleware

Call `next()`, do work before and after it:

```rust
Router::new()
    .middleware(async |c: &mut Ctx| {
        let start = std::time::Instant::now();
        c.next().await;
        println!("took {:?}", start.elapsed()); // after the handler runs
    })
    .get("/", async |_: &mut Ctx| "Hello!")
```

Or scope it to one route:

```rust
.get("/protected", (auth_middleware, async |c: &mut Ctx| {
    c.res.send("secret stuff");
}))
```

## Three Ways to Write Handlers

```rust
// Closure
.get("/", async |c: &mut Ctx| c.res.send("hi"))
.get("/", async |_: &mut Ctx| "hi")

// Function
async fn handler(c: &mut Ctx) -> &'static str {
    "hi"
}
.get("/", handler)

// Struct, for handlers that carry their own state
struct RateLimit { max: u32 }

impl Handler<&mut Ctx> for RateLimit {
    type Output = ();
    async fn call(&self, c: &mut Ctx) -> Self::Output {
        c.next().await;
    }
}

.middleware(RateLimit { max: 100 })
```

## Passing Extra Data Into a Handler

`WithState` lets you hand a closure some extra value without writing a struct for it:

```rust
.get("/users/{id}", WithState(db_pool.clone(), async |c: &mut Ctx, pool: DbPool| {
    let id: u32 = c.req.param("id")?;
    // use pool
    Ok(())
}))
```

No separate handler type, no trait impl, just a closure that takes one more argument. The value needs `Clone + Send + Sync + 'static`.

## Request Data

```rust
async |c: &mut Ctx| {
    let id: u32 = c.req.param("id")?;
    let q: MyQuery = c.req.query()?;
    let body: MyBody = c.req.json().await?;
    // .form(), and with the `xml` feature, .xml()
    // .parse() picks json/form/xml based on Content-Type
    Ok(())
}
```

`c.req.locals` is a typed per-request map (`AnyMap`) for stashing values between middleware and handlers. `c.res.locals` does the same for template context.

## Route Groups

```rust
let api = Router::group("/api")
    .get("/users", get_users)
    .post("/users", create_user);

let admin = Router::group("/admin")
    .middleware(require_admin)
    .get("/stats", stats);

Router::new()
    .push(api)
    .push(admin)
```

## Features

```toml
[dependencies]
maw = { version = "X.Y", features = ["minijinja", "websocket"] }
```

| Feature | What |
| ------- | ---- |
| `minijinja` | Template rendering |
| `xml` | XML request/response support |
| `websocket` | WebSocket support |
| `static_files` | Serve embedded files, with optional SPA fallback |
| `middleware-cookie` | Cookie parsing/setting (plain, signed, encrypted) |
| `middleware-session` | Session management, pluggable storage backend |
| `middleware-csrf` | CSRF protection (cookie or session backed) |
| `middleware-logging` | Request logging |
| `middleware-catch_panic` | Panic recovery, with optional custom handler |
| `middleware-body_limit` | Per-request body size limits |
| `middleware` | All middleware features |
| `full` | Everything |

Check `Cargo.toml` for the current version. Don't copy the `X.Y` above literally.

## Templates

[MiniJinja](https://github.com/mitsuhiko/minijinja):

```rust
let app = App::new()
    .views("templates")
    .router(Router::new()
        .get("/", async |c: &mut Ctx| {
            c.res.render("index.html");
        })
        .get("/user/{name}", async |c: &mut Ctx| {
            c.res.render_with("user.html", minijinja::context! {
                name => c.req.param_str("name")
            });
        })
    );
```

## Static Files

```rust
Router::new().static_files("/", StaticFiles::new(Assets).fallback_to("index.html"))
```

`fallback_to` serves that file when a path doesn't match anything embedded, useful for SPAs. `max_age` sets `Cache-Control`. `Last-Modified` / `If-Modified-Since` are handled for you.

## WebSocket

```rust
.ws("/ws", async |mut ws| {
    while let Some(Ok(msg)) = ws.recv().await {
        if let WsMessage::Text(txt) = msg {
            ws.send(format!("echo: {txt}")).await.ok();
        }
    }
})
```

## Server-Sent Events

```rust
use bytes::Bytes;
use futures_util::stream;

.get("/events", async |c: &mut Ctx| {
    let stream = stream::iter([
        Ok::<Bytes, std::convert::Infallible>(Bytes::from("data: hello\n\n")),
    ]);

    // sets SSE headers, closes automatically on app shutdown
    c.res.sse(stream);
})
```

## Errors

Return `Result<T, StatusError>` from a handler and Maw turns it into the right response:

```rust
async |c: &mut Ctx| -> Result<(), StatusError> {
    if !authorized {
        return Err(StatusError::forbidden());
    }
    Ok(())
}
```

`StatusError` has a constructor for every standard status code, plus `.brief()`, `.detail()`, and `.error()` to attach more context.

## Debug Your Routes

```rust
let router = Router::new()
    .middleware(logging)
    .push(api)
    .push(admin);

println!("{router:#?}");
```

## License

MIT
