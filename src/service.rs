//! Service abstraction for language servers.

use std::error::Error;
use std::fmt::{self, Debug, Display, Formatter, Result as FmtResult};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::channel::mpsc::{self, Receiver};
use futures::stream::FusedStream;
use futures::{future, FutureExt, Stream};
use log::{error, trace};
use tower_service::Service;

use super::client::Client;
use super::jsonrpc::{self, ClientRequests, Incoming, Outgoing, Response, ServerRequests};
use super::{generated_impl, LanguageServer};

/// Error that occurs when attempting to call the language server after it has already exited.
#[derive(Clone, Debug, PartialEq)]
pub struct ExitedError;

impl Display for ExitedError {
    fn fmt(&self, fmt: &mut Formatter) -> FmtResult {
        write!(fmt, "language server has exited")
    }
}

impl Error for ExitedError {}

/// Stream of messages produced by the language server.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct MessageStream(Receiver<Outgoing>);

impl Stream for MessageStream {
    type Item = Outgoing;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let recv = &mut self.as_mut().0;
        Pin::new(recv).poll_next(cx)
    }
}

impl FusedStream for MessageStream {
    #[inline]
    fn is_terminated(&self) -> bool {
        self.0.is_terminated()
    }
}

/// Service abstraction for the Language Server Protocol.
///
/// This service takes an incoming JSON-RPC message as input and produces an outgoing message as
/// output. If the incoming message is a server notification or a client response, then the
/// corresponding response will be `None`.
///
/// This implements [`tower_service::Service`] in order to remain independent from the underlying
/// transport and to facilitate further abstraction with middleware.
///
/// [`tower_service::Service`]: https://docs.rs/tower-service/0.3.0/tower_service/trait.Service.html
///
/// Pending requests can be canceled by issuing a [`$/cancelRequest`] notification.
///
/// [`$/cancelRequest`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#cancelRequest
///
/// The service shuts down and stops serving requests after the [`exit`] notification is received.
///
/// [`exit`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#exit
pub struct LspService {
    server: Arc<dyn LanguageServer>,
    pending_server: ServerRequests,
    pending_client: Arc<ClientRequests>,
    initialized: Arc<AtomicBool>,
    stopped: AtomicBool,
}

impl LspService {
    /// Creates a new `LspService` with the given server backend, also returning a stream of
    /// notifications from the server back to the client.
    pub fn new<T, F>(init: F) -> (Self, MessageStream)
    where
        F: FnOnce(Client) -> T,
        T: LanguageServer,
    {
        let initialized = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel(1);
        let messages = MessageStream(rx);

        let pending_client = Arc::new(ClientRequests::new());
        let client = Client::new(tx, pending_client.clone(), initialized.clone());

        let service = LspService {
            server: Arc::from(init(client)),
            pending_server: ServerRequests::new(),
            pending_client,
            initialized,
            stopped: AtomicBool::new(false),
        };

        (service, messages)
    }
}

impl Service<Incoming> for LspService {
    type Response = Option<Outgoing>;
    type Error = ExitedError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context) -> Poll<Result<(), Self::Error>> {
        if self.stopped.load(Ordering::SeqCst) {
            Poll::Ready(Err(ExitedError))
        } else {
            Poll::Ready(Ok(()))
        }
    }

    fn call(&mut self, request: Incoming) -> Self::Future {
        if self.stopped.load(Ordering::SeqCst) {
            future::err(ExitedError).boxed()
        } else {
            match request {
                Incoming::Request(req) => generated_impl::handle_request(
                    self.server.clone(),
                    &self.stopped,
                    self.initialized.clone(),
                    &self.pending_server,
                    req,
                ),
                Incoming::Response(res) => {
                    trace!("received client response: {:?}", res);
                    self.pending_client.insert(res);
                    future::ok(None).boxed()
                }
                Incoming::Invalid { id, method } => match (id, method) {
                    (None, Some(method)) if method.starts_with("$/") => future::ok(None).boxed(),
                    (Some(id), Some(method)) => {
                        error!("received invalid JSON message {:?} with ID: {}", method, id);
                        let res = Response::error(Some(id), jsonrpc::Error::method_not_found());
                        future::ok(Some(Outgoing::Response(res))).boxed()
                    }
                    (id, _) => {
                        match &id {
                            Some(id) => error!("received invalid JSON message with ID: {}", id),
                            None => error!("received invalid JSON message"),
                        }
                        let res = Response::error(id, jsonrpc::Error::invalid_request());
                        future::ok(Some(Outgoing::Response(res))).boxed()
                    }
                },
            }
        }
    }
}

impl Debug for LspService {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct(stringify!(LspService))
            .field("pending_server", &self.pending_server)
            .field("pending_client", &self.pending_client)
            .field("initialized", &self.initialized)
            .field("stopped", &self.stopped)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use lsp_types::*;
    use tower_test::mock::Spawn;

    use super::*;
    use crate::jsonrpc::Result;

    const INITIALIZE_REQUEST: &str =
        r#"{"jsonrpc":"2.0","method":"initialize","params":{"capabilities":{}},"id":1}"#;

    #[derive(Debug, Default)]
    struct Mock;

    #[async_trait]
    impl LanguageServer for Mock {
        async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
            Ok(InitializeResult::default())
        }

        async fn shutdown(&self) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn initializes_only_once() {
        let (service, _) = LspService::new(|_| Mock::default());
        let mut service = Spawn::new(service);

        let initialize: Incoming = serde_json::from_str(INITIALIZE_REQUEST).unwrap();
        let raw = r#"{"jsonrpc":"2.0","result":{"capabilities":{}},"id":1}"#;
        let ok = serde_json::from_str(raw).unwrap();
        assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
        assert_eq!(service.call(initialize.clone()).await, Ok(Some(ok)));

        let raw = r#"{"jsonrpc":"2.0","error":{"code":-32600,"message":"Invalid request"},"id":1}"#;
        let err = serde_json::from_str(raw).unwrap();
        assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
        assert_eq!(service.call(initialize).await, Ok(Some(err)));
    }

    #[tokio::test]
    async fn exit_notification() {
        let (service, _) = LspService::new(|_| Mock::default());
        let mut service = Spawn::new(service);

        let initialized: Incoming =
            serde_json::from_str(r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#)
                .unwrap();
        assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
        assert_eq!(service.call(initialized.clone()).await, Ok(None));

        let exit: Incoming = serde_json::from_str(r#"{"jsonrpc":"2.0","method":"exit"}"#).unwrap();
        assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
        assert_eq!(service.call(exit).await, Ok(None));

        assert_eq!(service.poll_ready(), Poll::Ready(Err(ExitedError)));
        assert_eq!(service.call(initialized).await, Err(ExitedError));
    }
}
