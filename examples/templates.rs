// Folder structure:
// src/main.rs

// templates/index.html:
/*
<!DOCTYPE html>
<html>

<head>
    <title>{{ app_name }}</title>
</head>

<body>
    <h1>Welcome to {{ app_name }} - {{ not_app_name }}</h1>
</body>

</html>
*/

// templates/user.html:
/*
<!DOCTYPE html>
<html>
<head>
    <title>{{ app_name }} - User Page</title>
</head>
<body>
    <h1>Welcome to {{ app_name }} - {{ not_app_name }}</h1>
    <h1>Hello, {{ name }}!</h1>
</body>
</html>
*/

use maw::prelude::*;

#[tokio::main]
async fn main() -> Result<(), MawError> {
    tracing_subscriber::fmt::init();

    // Uses the awesome https://crates.io/crates/minijinja as the template engine

    let app = App::new()
        .views_with("templates", |e| {
            e.add_global("app_name", "Maw - Minijinja");
            e.add_filter("upper", |s: &str| s.to_uppercase());
            e.add_function("lower", str::to_lowercase);
            e.add_global("test_value", 42);
        }) // folder with .html files
        .router(
            Router::new()
                .middleware(async |c: &mut Ctx| {
                    c.res.locals.insert("not_app_name", "some value");
                    c.next().await;
                })
                .get("/", async |c: &mut Ctx| {
                    c.res.render("index.html"); // templates/index.html
                })
                .get("/user/{name}", async |c: &mut Ctx| {
                    let name = c.req.param_str("name");
                    c.res.render_with("user.html", minijinja::context! { name });
                }),
        );

    app.listen("127.0.0.1:3000").await
}
