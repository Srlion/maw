use std::pin::Pin;

use crate::{handler::HandlerOutput, request::Request, response::Response};

async fn next(req: &mut Request, res: &mut Response) -> HandlerOutput {
    let handler = {
        let handler = req.handlers.middlewares.get(req.index_handler).cloned();
        req.index_handler += 1;
        handler
    };
    if let Some(handler) = handler {
        handler.run_next(req, res, NEXT).await
    } else {
        // call the method handler, either specific method or "all" handler
        let method = req
            .handlers
            .methods
            .get(req.method())
            .cloned()
            .or_else(|| req.handlers.all.clone())
            .expect("this should never happen, we check if a handler exists early");
        method.run(req, res).await
    }
}

pub type Next = for<'a> fn(
    &'a mut Request,
    &'a mut Response,
)
    -> Pin<Box<dyn std::future::Future<Output = HandlerOutput> + Send + 'a>>;

pub(crate) const NEXT: Next =
    |req: &mut Request, res: &mut Response| Box::pin(async move { next(req, res).await });
