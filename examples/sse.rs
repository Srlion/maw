use std::{convert::Infallible, time::Duration};

use bytes::Bytes;
use maw::prelude::*;

#[tokio::main]
async fn main() -> Result<(), MawError> {
    tracing_subscriber::fmt::init();

    let app = App::new().router(
        Router::new()
            .get("/events", sse_handler)
            .get("/events2", sse_handler_2),
    );

    app.listen("127.0.0.1:3000").await
}

pub async fn sse_handler(c: &mut Ctx) {
    c.res.sse(futures_util::stream::unfold(
        (tokio::time::interval(Duration::from_secs(1)), 0_u64),
        |(mut interval, count)| async move {
            interval.tick().await;
            let next = count + 1;
            let msg = format!("event: tick\ndata: {{\"count\": {next}}}\n\n");
            Some((Ok::<Bytes, Infallible>(Bytes::from(msg)), (interval, next)))
        },
    ));
}

pub async fn sse_handler_2(c: &mut Ctx) {
    c.res.sse(async_stream::stream! {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        let mut count = 0_u64;

        loop {
            interval.tick().await;
            count += 1;

            let msg = format!("event: tick\ndata: {{\"count\": {count}}}\n\n");
            yield Ok::<Bytes, Infallible>(Bytes::from(msg));
        }
    });
}
