//! Asynchronous `tower` server with an stdio transport.

use std::error::Error;

use futures::future::{Empty, IntoStream};
use futures::sync::mpsc;
use futures::{future, Future, Poll, Sink, Stream};
use log::error;
use tokio_codec::{FramedRead, FramedWrite};
use tokio_io::{AsyncRead, AsyncWrite};
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
    I: AsyncRead + Send + 'static,
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
    I: AsyncRead + Send + 'static,
    O: AsyncWrite + Send + 'static,
    S: Stream<Item = String, Error = ()> + Send + 'static,
{
    /// Interleaves the given stream of messages into `stdout` together with the responses.
    pub fn interleave<T>(self, stream: T) -> Server<I, O, T>
    where
        T: Stream<Item = String, Error = ()> + Send + 'static,
    {
        Server {
            stdin: self.stdin,
            stdout: self.stdout,
            interleave: stream,
        }
    }

    /// Spawns the service with messages read through `stdin` and responses printed to `stdout`.
    pub fn serve<T>(self, service: T) -> impl Future<Item = (), Error = ()> + Send
    where
        T: Service<Incoming, Response = String> + Send + 'static,
        T::Error: Into<Box<dyn Error + Send + Sync>>,
        T::Future: Send,
    {
        let (sender, receiver) = mpsc::channel(1);

        let framed_stdin = FramedRead::new(self.stdin, LanguageServerCodec::default());
        let framed_stdout = FramedWrite::new(self.stdout, LanguageServerCodec::default());
        let interleave = self.interleave;

        future::lazy(move || {
            let printer = receiver
                .select(interleave)
                .map_err(|_| error!("failed to log message"))
                .forward(framed_stdout.sink_map_err(|e| error!("failed to encode response: {}", e)))
                .map(|_| ());

            tokio_executor::spawn(printer);

            framed_stdin
                .map(Incoming::from)
                .map_err(|e| error!("failed to decode message: {}", e))
                .fold(service, move |mut service, line| {
                    let sender = sender.clone();
                    service
                        .call(line)
                        .map_err(|e| error!("{}", e.into()))
                        .and_then(move |resp| sender.send(resp).map_err(|_| unreachable!()))
                        .then(move |_| Ok(service))
                })
                .map(|_| ())
        })
    }
}

#[doc(hidden)]
#[derive(Debug)]
pub struct Nothing(IntoStream<Empty<String, ()>>);

impl Nothing {
    fn new() -> Self {
        Nothing(future::empty().into_stream())
    }
}

impl Stream for Nothing {
    type Item = String;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.0.poll()
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use futures::{future::FutureResult, stream, Async};
    use tokio::runtime::current_thread;

    use super::*;

    #[derive(Debug)]
    struct MockService;

    impl Service<Incoming> for MockService {
        type Response = String;
        type Error = String;
        type Future = FutureResult<Self::Response, Self::Error>;

        fn poll_ready(&mut self) -> Poll<(), Self::Error> {
            Ok(Async::Ready(()))
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

    // FIXME: Cannot inspect the input/output after serving because the server currently requires
    // the `stdin` and `stdout` handles to be `'static`, thereby requiring owned values and
    // disallowing `&` or `&mut` handles. This could potentially be fixed once async/await is
    // ready, or perhaps if we write a mock stdio type that is cloneable and has internal
    // synchronization.

    #[test]
    fn serves_on_stdio() {
        let (stdin, stdout) = mock_stdio();
        let server = Server::new(stdin, stdout).serve(MockService);
        current_thread::block_on_all(server).expect("failed to decode/encode message");
    }

    #[test]
    fn interleaves_messages() {
        let message = r#"{"jsonrpc":"2.0","method":"initialized"}"#.to_owned();
        let messages = stream::iter_ok(vec![message]);

        let (stdin, stdout) = mock_stdio();
        let server = Server::new(stdin, stdout)
            .interleave(messages)
            .serve(MockService);

        current_thread::block_on_all(server).expect("failed to decode/encode message");
    }
}
