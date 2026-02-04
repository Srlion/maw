use crate::{ctx::Ctx, handler::Handler};

pub struct BodyLimitMiddleware {
    max: usize,
}

impl BodyLimitMiddleware {
    pub fn new(max: usize) -> Self {
        Self { max }
    }
}

impl Handler<&mut Ctx> for BodyLimitMiddleware {
    type Output = ();

    async fn call(&self, c: &mut Ctx) -> Self::Output {
        c.req.set_body_limit(self.max);
        c.next().await;
    }
}
