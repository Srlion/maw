use std::path::PathBuf;

use cap_std::fs::Dir;
use http::StatusCode;
use http_body_util::StreamBody;
use tokio::fs::File;
use tokio_util::io::ReaderStream;

use crate::{async_fn::AsyncFn1, ctx::Ctx};

pub struct StaticMiddleware {
    dir: Dir,
    stream_threshold: u64,
}

impl StaticMiddleware {
    pub fn new(root: impl Into<PathBuf>) -> std::io::Result<Self> {
        Ok(Self {
            dir: Dir::open_ambient_dir(root.into(), cap_std::ambient_authority())?,
            stream_threshold: 1_048_576,
        })
    }

    pub fn with_stream_threshold(mut self, bytes: u64) -> Self {
        self.stream_threshold = bytes;
        self
    }
}

impl AsyncFn1<&mut Ctx> for StaticMiddleware {
    type Output = ();

    async fn call(&self, c: &mut Ctx) -> Self::Output {
        let path = c.req.uri().path().trim_start_matches('/');

        let metadata = match self.dir.metadata(path) {
            Ok(m) if m.is_file() => m,
            _ => {
                c.next().await;
                return;
            }
        };

        let mime = mime_guess::from_path(path).first_or_octet_stream();

        c.res.set(("Content-Type", mime.as_ref()));

        if metadata.len() > self.stream_threshold {
            let file = match self.dir.open(path) {
                Ok(f) => f,
                Err(_) => {
                    c.res.send_status(StatusCode::INTERNAL_SERVER_ERROR);
                    return;
                }
            };

            let stream = ReaderStream::new(File::from_std(file.into_std()));
            let body = StreamBody::new(stream);
            c.res.stream(body);
        } else {
            match self.dir.read(path) {
                Ok(contents) => {
                    c.res.send(contents);
                }
                Err(_) => {
                    c.res.send_status(StatusCode::INTERNAL_SERVER_ERROR);
                }
            }
        }
    }
}
