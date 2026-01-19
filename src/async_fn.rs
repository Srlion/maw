// Thanks to https://github.com/steffahn/async_fn_traits

use std::future::Future;

macro_rules! impl_async_fn {
    ($n:tt, [$($arg:ident),*]) => {
        paste::paste! {
            #[allow(non_snake_case)]
            #[allow(unused)]
            pub trait [<AsyncFn $n>]<$($arg),*> {
                type Output: Send;

                fn call(&self, $($arg: $arg),*) -> impl Future<Output = Self::Output> + Send
                where
                    Self: Sync,;
            }

            #[allow(non_snake_case)]
            #[allow(unused)]
            impl<F, Fut, $($arg),*> [<AsyncFn $n>]<$($arg),*> for F
            where
                F: Fn($($arg),*) -> Fut + Send + Sync,
                Fut: Future + Send,
                Fut::Output: Send,
                $($arg: Send,)*
            {
                type Output = Fut::Output;

                fn call(&self, $($arg: $arg),*) -> impl Future<Output = Self::Output> + Send
                where
                    Self: Sync,
                {
                    async move { (self)($($arg),*).await }
                }
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
