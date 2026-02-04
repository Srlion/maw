// Thanks to https://github.com/steffahn/async_fn_traits

use std::{future::Future, sync::Arc};

use crate::app::App;

#[allow(non_snake_case)]
pub trait AsyncFn1<Ctx> {
    type Output;
    fn call(&self, c: Ctx) -> impl Future<Output = Self::Output> + Send;

    #[allow(unused_variables)]
    fn on_app_listen_mut(&self, app: &mut App) {}

    #[allow(unused_variables)]
    fn on_app_listen_arc(&self, app: &Arc<App>) {}

    fn state(&self) -> &dyn std::any::Any {
        &()
    }
}

impl<F: ?Sized, Fut, Ctx> AsyncFn1<Ctx> for F
where
    F: Fn(Ctx) -> Fut,
    Fut: Future + Send,
{
    type Output = Fut::Output;
    fn call(&self, c: Ctx) -> impl Future<Output = Self::Output> + Send {
        (self)(c)
    }
}
