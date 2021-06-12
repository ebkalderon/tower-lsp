//! `tower` server which multiplexes bidirectional traffic over one connection.

use std::error::Error;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::channel::mpsc;
use futures::future::{self, Either, FutureExt, TryFutureExt};
use futures::sink::SinkExt;
use futures::stream::{self, Empty, Stream, StreamExt};
use log::error;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{FramedRead, FramedWrite};
use tower_service::Service;

use super::codec::LanguageServerCodec;
use super::jsonrpc::{self, Id, Incoming, Outgoing, Response};

/// Server for processing requests and responses on standard I/O or TCP.
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
    ///
    /// # Examples
    ///
    /// Initialize with a standard I/O transport:
    ///
    /// ```no_run
    /// # async fn docs() -> std::io::Result<()> {
    /// # use tower_lsp::Server;
    /// let stdin = tokio::io::stdin();
    /// let stdout = tokio::io::stdout();
    /// let server = Server::new(stdin, stdout);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Initialize with a TCP transport:
    ///
    /// ```no_run
    /// # async fn docs() -> std::io::Result<()> {
    /// # use tower_lsp::Server;
    /// let mut listener = tokio::net::TcpListener::bind("127.0.0.1:9257").await?;
    /// let (stream, _) = listener.accept().await?;
    /// let (read, write) = tokio::io::split(stream);
    /// let server = Server::new(read, write);
    /// # Ok(())
    /// # }
    /// ```
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
    S: Stream<Item = Outgoing>,
{
    /// Interleaves the given stream of messages into `stdout` together with the responses.
    pub fn interleave<T>(self, stream: T) -> Server<I, O, T>
    where
        T: Stream<Item = Outgoing>,
    {
        Server {
            stdin: self.stdin,
            stdout: self.stdout,
            interleave: stream,
        }
    }

    /// Spawns the service with messages read through `stdin` and responses written to `stdout`.
    pub async fn serve<T>(self, mut service: T)
    where
        T: Service<Incoming, Response = Option<Outgoing>> + Send + 'static,
        T::Error: Into<Box<dyn Error + Send + Sync>>,
        T::Future: Send,
    {
        let (mut sender, receiver) = mpsc::channel(16);

        let mut framed_stdin = FramedRead::new(self.stdin, LanguageServerCodec::default());
        let framed_stdout = FramedWrite::new(self.stdout, LanguageServerCodec::default());
        let responses = receiver.buffered(4).filter_map(future::ready);
        let interleave = self.interleave.fuse();

        let printer = stream::select(responses, interleave)
            .map(Ok)
            .forward(framed_stdout.sink_map_err(|e| error!("failed to encode message: {}", e)))
            .map(|_| ());

        let reader = async move {
            while let Some(msg) = framed_stdin.next().await {
                let request = match msg {
                    Ok(req) => req,
                    Err(err) => {
                        error!("failed to decode message: {}", err);
                        let response = Response::error(Id::Null, jsonrpc::Error::parse_error());
                        let response_fut = future::ready(Some(Outgoing::Response(response)));
                        sender.send(Either::Right(response_fut)).await.unwrap();
                        continue;
                    }
                };

                if let Err(err) = future::poll_fn(|cx| service.poll_ready(cx)).await {
                    error!("{}", display_sources(err.into().as_ref()));
                    return;
                }

                let response_fut = service.call(request).unwrap_or_else(|err| {
                    error!("{}", display_sources(err.into().as_ref()));
                    None
                });

                sender.send(Either::Left(response_fut)).await.unwrap();
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
pub struct Nothing(Empty<Outgoing>);

impl Nothing {
    fn new() -> Self {
        Nothing(stream::empty())
    }
}

impl Stream for Nothing {
    type Item = Outgoing;

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

    const REQUEST: &'static str = r#"{"jsonrpc":"2.0","method":"initialize","params":{},"id":1}"#;
    const RESPONSE: &'static str = r#"{"jsonrpc":"2.0","result":{"capabilities":{}},"id":1}"#;

    #[derive(Debug)]
    struct MockService;

    impl Service<Incoming> for MockService {
        type Response = Option<Outgoing>;
        type Error = String;
        type Future = Ready<Result<Self::Response, Self::Error>>;

        fn poll_ready(&mut self, _: &mut Context) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _: Incoming) -> Self::Future {
            let value = serde_json::from_str(RESPONSE).unwrap();
            future::ok(Some(Outgoing::Response(value)))
        }
    }

    fn mock_request() -> Vec<u8> {
        format!("Content-Length: {}\r\n\r\n{}", REQUEST.len(), REQUEST).into_bytes()
    }

    fn mock_response() -> Vec<u8> {
        format!("Content-Length: {}\r\n\r\n{}", RESPONSE.len(), RESPONSE).into_bytes()
    }

    fn mock_stdio() -> (Cursor<Vec<u8>>, Vec<u8>) {
        (Cursor::new(mock_request()), Vec::new())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn serves_on_stdio() {
        let (mut stdin, mut stdout) = mock_stdio();
        Server::new(&mut stdin, &mut stdout)
            .serve(MockService)
            .await;

        assert_eq!(stdin.position(), 80);
        assert_eq!(stdout, mock_response());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn interleaves_messages() {
        let message = Outgoing::Response(serde_json::from_str(RESPONSE).unwrap());
        let messages = stream::iter(vec![message]);

        let (mut stdin, mut stdout) = mock_stdio();
        Server::new(&mut stdin, &mut stdout)
            .interleave(messages)
            .serve(MockService)
            .await;

        assert_eq!(stdin.position(), 80);
        let output: Vec<_> = mock_response().into_iter().chain(mock_response()).collect();
        assert_eq!(stdout, output);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn handles_invalid_json() {
        let invalid = r#"{"jsonrpc":"2.0","method":"#;
        let message = format!("Content-Length: {}\r\n\r\n{}", invalid.len(), invalid).into_bytes();
        let (mut stdin, mut stdout) = (Cursor::new(message), Vec::new());

        Server::new(&mut stdin, &mut stdout)
            .serve(MockService)
            .await;

        assert_eq!(stdin.position(), 48);
        let err = r#"{"jsonrpc":"2.0","error":{"code":-32700,"message":"Parse error"},"id":null}"#;
        let output = format!("Content-Length: {}\r\n\r\n{}", err.len(), err).into_bytes();
        assert_eq!(stdout, output);
    }
}
