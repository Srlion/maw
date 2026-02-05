# Maw

A minimal web framework for Rust. No macros, just functions.

[![GitHub stars](https://img.shields.io/github/stars/srlion/maw?style=social)](https://github.com/srlion/maw)
[![Sponsor](https://img.shields.io/badge/sponsor-❤-ff69b4)](https://github.com/sponsors/srlion)

## Why Maw?

- **No macros** - Rust already looks beautiful, no need to hide it. Some frameworks go heavy on type gymnastics—we'd rather keep things readable
- **Express-style `next()`** - Call `c.next().await`, then inspect/modify the response after. Most Rust frameworks don't let you do this
- **Debug your routes** - Print the router to see exactly what handlers run where, with source locations
- **Fast enough** - High RPS, competitive performance. Some APIs trade microseconds for ergonomics, but Rust is already faster than Go/Node anyway

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

Works like Express/Fiber. Call `next()`, do stuff before/after:

```rust
Router::new()
    .middleware(async |c: &mut Ctx| {
        let start = std::time::Instant::now();
        c.next().await;
        println!("took {:?}", start.elapsed()); // runs AFTER handler
    })
    .get("/", async |_: &mut Ctx| "Hello!")
```

Or attach middleware to specific routes:

```rust
.get("/protected", ((
    async |c: &mut Ctx| {
        let start = std::time::Instant::now();
        c.next().await;
        println!("took {:?}", start.elapsed()); // runs AFTER handler
    },
    async |c: &mut Ctx| {
        c.res.send("secret stuff");
    },
)))
```

```rust
.get("/protected", (auth_middleware, async |c: &mut Ctx| {
    c.res.send("secret stuff");
}))
```

## Three Ways to Write Handlers

```rust
// 1. Async closure
.get("/", async |c: &mut Ctx| c.res.send("hi"))
.get("/", async |_: &mut Ctx| "hi")

// 2. Function
async fn handler(c: &mut Ctx) {
    c.res.send("hi");
}
async fn handler(c: &mut Ctx) -> &'static str {
    "hi"
}
.get("/", handler)

// 3. Struct - implement Handler (AsyncFn1) for stateful handlers, you can
struct RateLimit { max: u32 }

impl Handler<&mut Ctx> for RateLimit {
    type Output = ();
    async fn call(&self, c: &mut Ctx) -> Self::Output {
        // check rate limit using self.max...
        c.next().await;
    }
}

.middleware(RateLimit { max: 100 })

//

struct Hello { name: String }

impl Handler<&mut Ctx> for Hello {
    type Output = ();
    async fn call(&self, c: &mut Ctx) -> Self::Output {
        c.res.send(format!("Hello, {}!", self.name));
    }
}

.get("/hello", Hello { name: "Alice".into() })
```

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

## Debug Your Routes

```rust
let router = Router::new()
    .middleware(logging)
    .push(api)
    .push(admin);

println!("{router:#?}");
// Shows every route, its handlers, and where they're defined
```

## Features

Enable what you need:

```toml
[dependencies]
maw = { version = "0.19", features = ["minijinja", "websocket"] }
```

| Feature | What |
| ------- | ---- |
| `minijinja` | Template rendering |
| `xml` | XML request/response support |
| `websocket` | WebSocket support |
| `static_files` | Serve embedded files |
| `middleware-cookie` | Cookie parsing/setting |
| `middleware-session` | Session management |
| `middleware-csrf` | CSRF protection |
| `middleware-logging` | Request logging |
| `middleware-catch_panic` | Panic recovery |
| `middleware-body_limit` | Request body size limits |
| `middleware` | All middleware features |
| `full` | Everything |

## Templates

Uses [MiniJinja](https://github.com/mitsuhiko/minijinja):

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

## License

MIT
