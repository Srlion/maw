// Thanks to https://github.com/steffahn/async_fn_traits

use std::future::Future;

macro_rules! impl_async_fn {
    ($n:tt, [$($arg:ident),*]) => {
        paste::paste! {
            #[allow(unused)]
            pub trait [<AsyncFn $n>]<$($arg),*>: Fn($($arg),*) -> Self::OutputFuture {
                type OutputFuture: Future<Output = <Self as [<AsyncFn $n>]<$($arg),*>>::Output> + Send;
                type Output;
            }

            impl<F: ?Sized, Fut, $($arg),*> [<AsyncFn $n>]<$($arg),*> for F
            where
                F: Fn($($arg),*) -> Fut,
                Fut: Future + Send,
            {
                type OutputFuture = Fut;
                type Output = Fut::Output;
            }
        }
    };
}

impl_async_fn!(0, []);
impl_async_fn!(1, [Arg0]);
impl_async_fn!(2, [Arg0, Arg1]);
impl_async_fn!(3, [Arg0, Arg1, Arg2]);
impl_async_fn!(4, [Arg0, Arg1, Arg2, Arg3]);
impl_async_fn!(5, [Arg0, Arg1, Arg2, Arg3, Arg4]);
