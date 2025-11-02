use std::sync::Arc;

use crate::{
    app::App,
    handler::{Handler, HandlerOutput},
    request::Request,
    response::Response,
};

pub struct Ctx {
    pub req: Request,
    pub res: Response,
    pub(crate) handlers: Arc<[Handler]>,
    pub(crate) index_handler: usize,
}

impl Ctx {
    pub(crate) fn new(req: Request, res: Response, handlers: Arc<[Handler]>) -> Self {
        Self {
            req,
            res,
            handlers,
            index_handler: 0,
        }
    }

    #[inline]
    pub fn app(&self) -> &App {
        self.req.app()
    }

    pub async fn next(&mut self) -> HandlerOutput {
        while let Some(handler) = self.handlers.get(self.index_handler) {
            self.index_handler += 1;

            match handler.handler_type() {
                crate::handler::HandlerType::Middleware { .. } => {
                    return handler.clone().run(self).await;
                }
                crate::handler::HandlerType::Method {
                    method,
                    use_as_head,
                } => {
                    let matches = match (self.req.method(), method) {
                        (a, b) if a == b => true,
                        (&http::Method::HEAD, &http::Method::GET) => {
                            use_as_head.load(std::sync::atomic::Ordering::Relaxed)
                        }
                        _ => false,
                    };
                    if matches {
                        // If middleware hasn't set a status code, default to 200 OK
                        // This is because by default we set 404 Not Found in the app handler
                        if !self.res.status_modified {
                            self.res.status(http::StatusCode::OK);
                        }
                        self.index_handler = self.handlers.len() + 1; // skip remaining handlers, as we found our method handler
                        return handler.clone().run(self).await;
                    }
                    // If method doesn't match, continue to next iteration
                }
            }
        }
        Ok(())
    }
}
