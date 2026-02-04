use crate::{async_fn::Handler, ctx::Ctx};

pub struct LoggingMiddleware {
    _p: (),
}

impl LoggingMiddleware {
    pub fn new() -> Self {
        Self { _p: () }
    }
}

struct FormattedDuration(std::time::Duration);

impl std::fmt::Display for FormattedDuration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let nanos = self.0.as_nanos() as u64;
        if nanos >= 1_000_000_000 {
            let ms = nanos / 1_000_000;
            write!(f, "{}.{:03}s", ms / 1000, ms % 1000)
        } else if nanos >= 1_000_000 {
            let us = nanos / 1_000;
            write!(f, "{}.{:03}ms", us / 1000, us % 1000)
        } else {
            write!(f, "{}.{:03}µs", nanos / 1000, nanos % 1000)
        }
    }
}

impl Handler<&mut Ctx> for LoggingMiddleware {
    type Output = ();

    async fn call(&self, c: &mut Ctx) -> Self::Output {
        let time = std::time::Instant::now();

        c.next().await;

        tracing::info!(
            "{} | {:^10} | {} | {:^7} | {}",
            c.res.inner.status().as_u16(),
            FormattedDuration(time.elapsed()),
            c.req.ip(),
            c.req.method().as_str(),
            c.req.uri().path(),
        );
    }
}
