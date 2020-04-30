//! Asynchronous `tower` server with an stdio transport.

use std::error::Error;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::channel::mpsc;
use futures::future::{self, FutureExt};
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
    I: AsyncRead + Unpin,
    O: AsyncWrite,
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
    I: AsyncRead + Unpin,
    O: AsyncWrite,
    S: Stream<Item = String>,
{
    /// Interleaves the given stream of messages into `stdout` together with the responses.
    pub fn interleave<T>(self, stream: T) -> Server<I, O, T>
    where
        T: Stream<Item = String>,
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
        T: Service<Incoming, Response = Option<String>> + Send + 'static,
        T::Error: Into<Box<dyn Error + Send + Sync>>,
        T::Future: Send,
    {
        let (mut sender, receiver) = mpsc::channel(1);

        let mut framed_stdin = FramedRead::new(self.stdin, LanguageServerCodec::default());
        let framed_stdout = FramedWrite::new(self.stdout, LanguageServerCodec::default());
        let responses = receiver.buffer_unordered(4).filter_map(future::ready);
        let interleave = self.interleave.fuse();

        let printer = stream::select(responses, interleave)
            .map(Ok)
            .forward(framed_stdout.sink_map_err(|e| error!("failed to encode response: {}", e)))
            .map(|_| ());

        let reader = async move {
            while let Some(line) = framed_stdin.next().await {
                let request = match line {
                    Ok(req) => Incoming::from(req),
                    Err(err) => {
                        error!("failed to decode message: {}", err);
                        continue;
                    }
                };

                if let Err(err) = future::poll_fn(|cx| service.poll_ready(cx)).await {
                    error!("{}", display_sources(err.into().as_ref()));
                    return;
                }

                let fut = service.call(request);
                let response_fut = async move {
                    match fut.await {
                        Ok(Some(res)) => Some(res),
                        Ok(None) => None,
                        Err(err) => {
                            error!("{}", display_sources(err.into().as_ref()));
                            None
                        }
                    }
                };

                sender.send(response_fut).await.unwrap();
            }
        };

        futures::join!(reader, printer);
    }
}

fn display_sources(error: &dyn Error) -> String {
    if let Some(source) = error.source() {
        format!("{}: {}", error, display_sources(source))
    } else {
        error.to_string()
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
        type Response = Option<String>;
        type Error = String;
        type Future = Ready<Result<Self::Response, Self::Error>>;

        fn poll_ready(&mut self, _: &mut Context) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, request: Incoming) -> Self::Future {
            future::ok(Some(request.to_string()))
        }
    }

    fn mock_request() -> Vec<u8> {
        let message = r#"{"jsonrpc":"2.0","method":"initialized","params":null}"#;
        format!("Content-Length: {}\r\n\r\n{}", message.len(), message).into_bytes()
    }

    fn mock_stdio() -> (Cursor<Vec<u8>>, Vec<u8>) {
        (Cursor::new(mock_request()), Vec::new())
    }

    #[tokio::test]
    async fn serves_on_stdio() {
        let (mut stdin, mut stdout) = mock_stdio();
        Server::new(&mut stdin, &mut stdout)
            .serve(MockService)
            .await;

        assert_eq!(stdin.position(), 76);
        assert_eq!(stdout, mock_request());
    }

    #[tokio::test]
    async fn interleaves_messages() {
        let message = r#"{"jsonrpc":"2.0","method":"initialized","params":null}"#.to_owned();
        let messages = stream::iter(vec![message]);

        let (mut stdin, mut stdout) = mock_stdio();
        Server::new(&mut stdin, &mut stdout)
            .interleave(messages)
            .serve(MockService)
            .await;

        assert_eq!(stdin.position(), 76);
        let output: Vec<_> = mock_request().into_iter().chain(mock_request()).collect();
        assert_eq!(stdout, output);
    }
}
