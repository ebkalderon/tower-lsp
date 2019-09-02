//! Service abstraction for language servers.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use futures::future::{self, Future, Shared, SharedError, SharedItem};
use futures::sync::oneshot::{self, Canceled};
use futures::{Async, Poll};
use jsonrpc_core::IoHandler;
use log::{info, trace};
use lsp_types::notification::{Exit, Notification};
use tower_service::Service;

use super::delegate::{Delegate, LanguageServerCore, MessageStream};
use super::LanguageServer;

/// Future which never resolves until the [`exit`] notification is received.
///
/// [`exit`]: https://microsoft.github.io/language-server-protocol/specification#exit
#[derive(Clone, Debug)]
pub struct ExitReceiver(Shared<oneshot::Receiver<()>>);

impl ExitReceiver {
    /// Drives the future to completion, only canceling if the [`exit`] notification is received.
    ///
    /// [`exit`]: https://microsoft.github.io/language-server-protocol/specification#exit
    pub fn run_until_exit<F>(self, future: F) -> impl Future<Item = (), Error = ()> + Send + 'static
    where
        F: Future<Item = (), Error = ()> + Send + 'static,
    {
        self.0.then(|_| Ok(())).select(future).then(|_| Ok(()))
    }
}

impl Future for ExitReceiver {
    type Item = SharedItem<()>;
    type Error = SharedError<Canceled>;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.0.poll()
    }
}

/// Service abstraction for the Language Server Protocol.
///
/// This service takes a JSON-RPC request as input and produces a JSON-RPC response as output. If
/// the incoming request is a notification, then the corresponding response string will be empty.
///
/// This implements [`tower_service::Service`] in order to remain independent from the underlying
/// transport and to facilitate further abstraction with middleware.
///
/// [`tower_service::Service`]: https://docs.rs/tower-service/0.2.0/tower_service/trait.Service.html
#[derive(Debug)]
pub struct LspService {
    handler: IoHandler,
    exit_rx: ExitReceiver,
    stopped: Arc<AtomicBool>,
}

impl LspService {
    /// Creates a new `LspService` with the given server backend, also returning a stream of
    /// notifications from the server back to the client.
    pub fn new<T>(server: T) -> (Self, MessageStream)
    where
        T: LanguageServer,
    {
        Self::with_handler(server, IoHandler::new())
    }

    /// Creates a new `LspService` with the given server backend a custom `IoHandler`.
    pub fn with_handler<T, U>(server: T, handler: U) -> (Self, MessageStream)
    where
        T: LanguageServer,
        U: Into<IoHandler>,
    {
        let (delegate, messages) = Delegate::new(server);

        let mut handler = handler.into();
        handler.extend_with(delegate.to_delegate());

        let (tx, rx) = oneshot::channel();
        let exit_tx = Mutex::new(Some(tx));
        let exit_rx = ExitReceiver(rx.shared());

        let stopped = Arc::new(AtomicBool::new(false));
        let stopped_arc = stopped.clone();
        handler.add_notification(Exit::METHOD, move |_| {
            if let Some(tx) = exit_tx.lock().unwrap_or_else(|tx| tx.into_inner()).take() {
                info!("exit notification received, shutting down");
                stopped_arc.store(true, Ordering::SeqCst);
                tx.send(()).unwrap();
            }
        });

        let service = LspService {
            handler,
            exit_rx,
            stopped,
        };

        (service, messages)
    }

    /// Returns a close handle which signals when the [`exit`] notification has been received.
    ///
    /// [`exit`]: https://microsoft.github.io/language-server-protocol/specification#exit
    pub fn close_handle(&self) -> ExitReceiver {
        self.exit_rx.clone()
    }
}

impl Service<String> for LspService {
    type Response = String;
    type Error = ();
    type Future = Box<dyn Future<Item = Self::Response, Error = Self::Error> + Send>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        if self.stopped.load(Ordering::SeqCst) {
            Ok(Async::NotReady)
        } else {
            Ok(Async::Ready(()))
        }
    }

    fn call(&mut self, request: String) -> Self::Future {
        if self.stopped.load(Ordering::SeqCst) {
            Box::new(future::err(()))
        } else {
            Box::new(self.handler.handle_request(&request).map(move |result| {
                result.unwrap_or_else(|| {
                    trace!("request produced no response: {}", request);
                    String::new()
                })
            }))
        }
    }
}
