use crate::ctx::Ctx;

pub fn middleware(
    max: usize,
) -> impl Fn(&mut Ctx) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> + Clone
{
    move |c: &mut Ctx| {
        Box::pin(async move {
            c.req.body_limit(max);
            c.next().await;
        })
    }
}
