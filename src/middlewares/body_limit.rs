use crate::{async_fn::AsyncFn1, ctx::Ctx};

pub struct BodyLimitMiddleware {
    max: usize,
}

impl BodyLimitMiddleware {
    pub fn new(max: usize) -> Self {
        Self { max }
    }
}

impl AsyncFn1<&mut Ctx> for BodyLimitMiddleware {
    type Output = ();

    async fn call(&self, c: &mut Ctx) -> Self::Output {
        c.req.set_body_limit(self.max);
        c.next().await;
    }
}
