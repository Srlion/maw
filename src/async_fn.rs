// Thanks to https://github.com/steffahn/async_fn_traits

use std::future::Future;

macro_rules! impl_async_fn {
    ($n:tt, [$($arg:ident),*]) => {
        paste::paste! {
            #[allow(non_snake_case)]
            pub trait [<AsyncFn $n>]<$($arg),*> {
                type Output;
                type Future: Future<Output = Self::Output> + Send;

                fn call(&self, $($arg: $arg),*) -> Self::Future;
            }

            #[allow(non_snake_case)]
            impl<F: ?Sized, Fut, $($arg),*> [<AsyncFn $n>]<$($arg),*> for F
            where
                F: Fn($($arg),*) -> Fut,
                Fut: Future + Send,
            {
                type Output = Fut::Output;
                type Future = Fut;

                fn call(&self, $($arg: $arg),*) -> Self::Future {
                    (self)($($arg),*)
                }
            }
        }
    };
}

impl_async_fn!(1, [Arg0]);
impl_async_fn!(2, [Arg0, Arg1]);
