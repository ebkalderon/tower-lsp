//! Asynchronous `tower` server with an stdio transport.

use std::error::Error;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::channel::mpsc;
use futures::future::FutureExt;
use futures::sink::SinkExt;
use futures::stream::{self, Empty, Stream, StreamExt};
use log::error;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{FramedRead, FramedWrite};
use tower_service::Service;

use super::codec::LanguageServerCodec;
use super::message::Incoming;

/// Server for processing requests and responses on `stdin` and `stdout`.
#[derive(Debug)]
pub struct Server<I, O, S = Nothing> {
    stdin: I,
    stdout: O,
    interleave: S,
}

impl<I, O> Server<I, O, Nothing>
where
    I: AsyncRead + Send + Unpin,
    O: AsyncWrite + Send + 'static,
{
    /// Creates a new `Server` with the given `stdin` and `stdout` handles.
    pub fn new(stdin: I, stdout: O) -> Self {
        Server {
            stdin,
            stdout,
            interleave: Nothing::new(),
        }
    }
}

impl<I, O, S> Server<I, O, S>
where
    I: AsyncRead + Send + Unpin,
    O: AsyncWrite + Send + 'static,
    S: Stream<Item = String> + Send + 'static,
{
    /// Interleaves the given stream of messages into `stdout` together with the responses.
    pub fn interleave<T>(self, stream: T) -> Server<I, O, T>
    where
        T: Stream<Item = String> + Send + 'static,
    {
        Server {
            stdin: self.stdin,
            stdout: self.stdout,
            interleave: stream,
        }
    }

    /// Spawns the service with messages read through `stdin` and responses printed to `stdout`.
    pub async fn serve<T>(self, mut service: T)
    where
        T: Service<Incoming, Response = String> + Send + 'static,
        T::Error: Into<Box<dyn Error + Send + Sync>>,
        T::Future: Send,
    {
        let (mut sender, receiver) = mpsc::channel(1);

        let mut framed_stdin = FramedRead::new(self.stdin, LanguageServerCodec::default());
        let framed_stdout = FramedWrite::new(self.stdout, LanguageServerCodec::default());
        let interleave = self.interleave.fuse();

        let printer = stream::select(receiver, interleave)
            .map(Ok)
            .forward(framed_stdout.sink_map_err(|e| error!("failed to encode response: {}", e)))
            .map(|_| ());

        tokio::spawn(printer);

        while let Some(line) = framed_stdin.next().await {
            let request = match line.map(Incoming::from) {
                Ok(request) => request,
                Err(err) => {
                    error!("failed to decode message: {}", err);
                    continue;
                }
            };

            match service.call(request).await {
                Ok(resp) => sender.send(resp).await.unwrap(),
                Err(err) => error!("{}", err.into()),
            }
        }
    }
}

#[doc(hidden)]
#[derive(Debug)]
pub struct Nothing(Empty<String>);

impl Nothing {
    fn new() -> Self {
        Nothing(stream::empty())
    }
}

impl Stream for Nothing {
    type Item = String;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let stream = &mut self.as_mut().0;
        Pin::new(stream).poll_next(cx)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use futures::future::Ready;
    use futures::{future, stream};

    use super::*;

    #[derive(Debug)]
    struct MockService;

    impl Service<Incoming> for MockService {
        type Response = String;
        type Error = String;
        type Future = Ready<Result<Self::Response, Self::Error>>;

        fn poll_ready(&mut self, _: &mut Context) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, request: Incoming) -> Self::Future {
            future::ok(request.to_string())
        }
    }

    fn mock_stdio() -> (Cursor<Box<[u8]>>, Cursor<Box<[u8]>>) {
        let message = r#"{"jsonrpc":"2.0","method":"initialized"}"#;
        let stdin = format!("Content-Length: {}\r\n\r\n{}", message.len(), message);
        (
            Cursor::new(stdin.into_bytes().into_boxed_slice()),
            Cursor::new(Box::new([])),
        )
    }

    // FIXME: Cannot inspect the output after serving because the server currently requires that
    // `stdout` be `'static`, thereby requiring owned values and disallowing `&mut` handles. This
    // could be fixed by spawning the `printer` in `Server::serve()` using a `LocalSet` once it
    // gains the ability to spawn non-`'static` futures. See the following issue for details:
    //
    // https://github.com/tokio-rs/tokio/issues/2013

    #[tokio::test]
    async fn serves_on_stdio() {
        let (mut stdin, stdout) = mock_stdio();
        Server::new(&mut stdin, stdout).serve(MockService).await;
        assert_eq!(stdin.position(), 62);
    }

    #[tokio::test]
    async fn interleaves_messages() {
        let message = r#"{"jsonrpc":"2.0","method":"initialized"}"#.to_owned();
        let messages = stream::iter(vec![message]);

        let (mut stdin, stdout) = mock_stdio();
        Server::new(&mut stdin, stdout)
            .interleave(messages)
            .serve(MockService)
            .await;

        assert_eq!(stdin.position(), 62);
    }
}
