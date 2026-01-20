use crate::{async_fn::AsyncFn1, ctx::Ctx};

pub struct LoggingMiddleware {
    _p: (),
}

impl LoggingMiddleware {
    pub fn new() -> Self {
        Self { _p: () }
    }
}

fn format_duration(d: std::time::Duration) -> String {
    let secs = d.as_secs_f64();

    if secs >= 1.0 {
        format!("{:.9}s", secs)
    } else if secs >= 0.001 {
        format!("{:.3}ms", secs * 1000.0)
    } else {
        format!("{:.3}µs", secs * 1_000_000.0)
    }
}

impl AsyncFn1<&mut Ctx> for LoggingMiddleware {
    type Output = ();

    async fn call(&self, c: &mut Ctx) -> Self::Output {
        let time = std::time::Instant::now();

        c.next().await;

        let duration = time.elapsed();

        tracing::info!(
            "{} | {:^10} | {} | {:^7} | {}",
            c.res.inner.status().as_u16(),
            format_duration(duration),
            c.req.ip(),
            c.req.method().as_str(),
            c.req.uri().path(),
        );
    }
}
