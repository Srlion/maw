use std::sync::Arc;

use crate::{app::App, handler::Handler, request::Request, response::Response};

pub struct Ctx {
    pub req: Request,
    pub res: Response,
    pub(crate) handlers: Arc<[Handler]>,
    pub(crate) index_handler: usize,
    #[cfg(feature = "cookie")]
    pub cookies: crate::middlewares::cookie::CookieStore,
    #[cfg(feature = "session")]
    pub session: crate::middlewares::session::SessionStore,
}

impl Ctx {
    pub(crate) fn new(req: Request, res: Response, handlers: Arc<[Handler]>) -> Self {
        Self {
            req,
            res,
            handlers,
            index_handler: 0,
            #[cfg(feature = "cookie")]
            cookies: Default::default(),
            #[cfg(feature = "session")]
            session: Default::default(),
        }
    }

    #[inline]
    pub fn handlers(&self) -> &[Handler] {
        &self.handlers
    }

    #[inline]
    pub fn current_handler_index(&self) -> usize {
        self.index_handler
    }

    #[inline]
    pub fn app(&self) -> &App {
        self.req.app()
    }

    pub async fn next(&mut self) {
        if let Some(handler) = self.handlers.get(self.index_handler) {
            self.index_handler += 1;
            return handler.clone().run(self).await;
        }
    }
}
