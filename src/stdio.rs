//! Asynchronous `tower` server with an stdio transport.

use futures::future::{Empty, IntoStream};
use futures::sync::mpsc;
use futures::{future, Future, Poll, Sink, Stream};
use log::error;
use tokio_codec::{FramedRead, FramedWrite};
use tokio_io::{AsyncRead, AsyncWrite};
use tower_service::Service;

use super::codec::LanguageServerCodec;

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
    pub fn serve<T>(self, service: T) -> impl Future<Item = (), Error = ()> + Send + 'static
    where
        T: Service<String, Response = String> + Send + 'static,
        T::Future: Send + 'static,
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
                .map_err(|e| error!("failed to decode request: {}", e))
                .fold(service, move |mut service, line| {
                    let sender = sender.clone();
                    service
                        .call(line)
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
