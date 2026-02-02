use maw::prelude::*;

// Embed static files at compile time using rust-embed
// Files are served with proper MIME types, caching, and Last-Modified headers
// rust_embed exists in prelude for ease of use

#[derive(rust_embed::RustEmbed)]
#[folder = "examples/assets/"]
struct Assets;

#[tokio::main]
async fn main() -> Result<(), MawError> {
    tracing_subscriber::fmt::init();

    let router = Router::new()
        .get("/", async |_: &mut Ctx| "Home")
        // Serve files from /static/* -> assets/
        .static_files("/static", StaticFiles::new(Assets).max_age(3600));

    let app = App::new().router(router);

    // GET /static/style.css  -> assets/style.css
    // GET /static/js/app.js  -> assets/js/app.js
    // GET /static/           -> assets/index.html

    app.listen("127.0.0.1:3000").await
}
