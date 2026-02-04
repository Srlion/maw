use maw::prelude::*;

#[tokio::main]
async fn main() -> Result<(), MawError> {
    tracing_subscriber::fmt::init();

    let app = App::new().router(
        Router::new()
            // Basic echo server
            .ws("/ws", async |mut ws| {
                while let Some(Ok(msg)) = ws.recv().await {
                    match msg {
                        WsMessage::Text(txt) => {
                            ws.send(format!("Echo: {txt}")).await.ok();
                        }
                        WsMessage::Binary(data) => {
                            ws.send(data).await.ok();
                        }
                        WsMessage::Close(_) => break,
                        _ => {}
                    }
                }
            })
            // this ^ is equivalent to this v, use the latter if you need access to the request context
            .get("/ws2", async |c: &mut Ctx| {
                c.upgrade_websocket(async move |mut ws| {
                    while let Some(Ok(msg)) = ws.recv().await {
                        match msg {
                            WsMessage::Text(txt) => {
                                ws.send(format!("Echo: {txt}")).await.ok();
                            }
                            WsMessage::Binary(data) => {
                                ws.send(data).await.ok();
                            }
                            WsMessage::Close(_) => break,
                            _ => {}
                        }
                    }
                })?;
                Ok(())
            }),
    );

    app.listen("127.0.0.1:3000").await
}
