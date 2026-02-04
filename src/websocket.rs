use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use futures_util::{Sink, Stream};
use http::Method;
use hyper_tungstenite::{
    HyperWebsocket,
    tungstenite::{self, protocol::CloseFrame},
};

pub use tungstenite::Message;

use crate::{
    ctx::Ctx,
    prelude::StatusError,
    request::Request,
    router::{AddHandlers as _, Router, WithState},
};

pub type WsError = tungstenite::Error;

#[derive(Debug, thiserror::Error)]
pub enum WsUpgradeError {
    #[error("not a websocket upgrade request")]
    NotWebSocket,
    #[error("websocket upgrade failed: {0}")]
    Upgrade(#[from] hyper_tungstenite::tungstenite::error::ProtocolError),
}

impl From<WsUpgradeError> for StatusError {
    fn from(e: WsUpgradeError) -> Self {
        match e {
            WsUpgradeError::NotWebSocket => {
                StatusError::bad_request().brief("Expected WebSocket upgrade request")
            }
            WsUpgradeError::Upgrade(e) => {
                StatusError::bad_request().brief(format!("WebSocket upgrade failed: {e}"))
            }
        }
    }
}

pub struct WebSocket {
    inner: hyper_tungstenite::WebSocketStream<hyper_util::rt::TokioIo<hyper::upgrade::Upgraded>>,
}

impl WebSocket {
    pub(crate) async fn from_hyper(ws: HyperWebsocket) -> Result<Self, WsError> {
        Ok(Self { inner: ws.await? })
    }

    pub async fn recv(&mut self) -> Option<Result<Message, WsError>> {
        use futures_util::StreamExt;
        self.inner.next().await
    }

    pub async fn send(&mut self, msg: impl Into<Message>) -> Result<(), WsError> {
        use futures_util::SinkExt;
        self.inner.send(msg.into()).await
    }

    pub async fn close(&mut self, frame: Option<CloseFrame>) -> Result<(), WsError> {
        self.inner.close(frame).await
    }
}

impl Stream for WebSocket {
    type Item = Result<Message, WsError>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

impl Sink<Message> for WebSocket {
    type Error = WsError;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.inner).poll_ready(cx)
    }

    fn start_send(mut self: Pin<&mut Self>, item: Message) -> Result<(), Self::Error> {
        Pin::new(&mut self.inner).start_send(item)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.inner).poll_close(cx)
    }
}

impl Request {
    pub fn is_websocket(&self) -> bool {
        // Check upgrade headers manually since we split the request
        self.headers()
            .get(http::header::UPGRADE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.eq_ignore_ascii_case("websocket"))
    }
}

impl Ctx {
    pub fn is_websocket(&self) -> bool {
        self.req.is_websocket()
    }

    pub fn upgrade_websocket<F, Fut>(&mut self, handler: F) -> Result<(), WsUpgradeError>
    where
        F: FnOnce(WebSocket) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send,
    {
        if !self.req.is_websocket() {
            return Err(WsUpgradeError::NotWebSocket);
        }

        let mut request = http::Request::new(());
        *request.headers_mut() = self.req.parts.headers.clone();
        *request.extensions_mut() = std::mem::take(&mut self.req.parts.extensions);

        let (response, ws_future) = hyper_tungstenite::upgrade(&mut request, None)?;
        let (parts, _) = response.into_parts();
        self.res.inner = http::Response::from_parts(parts, crate::response::HttpBody::Empty);

        tokio::spawn(async move {
            match WebSocket::from_hyper(ws_future).await {
                Ok(ws) => handler(ws).await,
                Err(e) => tracing::error!("WebSocket connection failed: {e}"),
            }
        });

        Ok(())
    }
}

impl Router {
    pub fn ws<F, Fut>(&self, path: impl Into<String>, handler: F) -> Self
    where
        F: Fn(WebSocket) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send,
    {
        WithState(Arc::new(handler), async |c: &mut Ctx, h: Arc<F>| {
            c.upgrade_websocket(move |ws| h(ws))?;
            Ok(())
        })
        .add_handlers(self, Method::GET, path, 5)
    }
}
