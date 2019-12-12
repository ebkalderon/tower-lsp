//! Service abstraction for language servers.

use std::error::Error;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use futures::future::{self, Future, Shared, SharedError, SharedItem};
use futures::sync::oneshot::{self, Canceled};
use futures::{Async, Poll};
use jsonrpc_core::IoHandler;
use log::{debug, info, trace};
use lsp_types::notification::{Exit, Notification};
use tower_service::Service;

use super::delegate::{Delegate, LanguageServerCore, MessageStream};
use super::message::Incoming;
use super::LanguageServer;

/// Error that occurs when attempting to call the language server after it has already exited.
#[derive(Clone, Debug, PartialEq)]
pub struct ExitedError;

impl Display for ExitedError {
    fn fmt(&self, fmt: &mut Formatter) -> FmtResult {
        write!(fmt, "language server has exited")
    }
}

impl Error for ExitedError {}

/// Future which never resolves until the [`exit`] notification is received.
///
/// [`exit`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#exit
#[derive(Clone, Debug)]
pub struct ExitReceiver(Shared<oneshot::Receiver<()>>);

impl ExitReceiver {
    /// Drives the future to completion, only canceling if the [`exit`] notification is received.
    ///
    /// [`exit`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#exit
    pub fn run_until_exit<F>(self, future: F) -> impl Future<Item = (), Error = ()> + Send
    where
        F: Future<Item = (), Error = ()> + Send,
    {
        self.0
            .then(|_| Ok(()))
            .select(future)
            .map(|item| item.0)
            .map_err(|err| err.0)
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
    /// [`exit`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#exit
    pub fn close_handle(&self) -> ExitReceiver {
        self.exit_rx.clone()
    }
}

impl Service<Incoming> for LspService {
    type Response = String;
    type Error = ExitedError;
    type Future = Box<dyn Future<Item = Self::Response, Error = Self::Error> + Send>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        if self.stopped.load(Ordering::SeqCst) {
            Ok(Async::NotReady)
        } else {
            Ok(Async::Ready(()))
        }
    }

    fn call(&mut self, request: Incoming) -> Self::Future {
        if self.stopped.load(Ordering::SeqCst) {
            Box::new(future::err(ExitedError))
        } else {
            if let Incoming::Response(r) = request {
                // FIXME: Currently, we are dropping responses to requests created in `Printer`.
                // We need some way to route them back to the `Printer`. See this issue for more:
                //
                // https://github.com/ebkalderon/tower-lsp/issues/13
                debug!("dropping client response, as per GitHub issue #13: {:?}", r);
                Box::new(future::ok(String::new()))
            } else {
                Box::new(
                    self.handler
                        .handle_request(&request.to_string())
                        .map_err(|_| unreachable!())
                        .map(move |result| {
                            result.unwrap_or_else(|| {
                                trace!("request produced no response: {}", request);
                                String::new()
                            })
                        }),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use jsonrpc_core::{BoxFuture, Result};
    use lsp_types::*;
    use serde_json::Value;

    use super::*;
    use crate::Printer;

    #[derive(Debug, Default)]
    struct Mock;

    impl LanguageServer for Mock {
        type ShutdownFuture = BoxFuture<()>;
        type SymbolFuture = BoxFuture<Option<Vec<SymbolInformation>>>;
        type ExecuteFuture = BoxFuture<Option<Value>>;
        type CompletionFuture = BoxFuture<Option<CompletionResponse>>;
        type HighlightFuture = BoxFuture<Option<Vec<DocumentHighlight>>>;
        type HoverFuture = BoxFuture<Option<Hover>>;

        fn initialize(&self, _: &Printer, _: InitializeParams) -> Result<InitializeResult> {
            Ok(InitializeResult::default())
        }

        fn shutdown(&self) -> Self::ShutdownFuture {
            Box::new(future::ok(()))
        }

        fn symbol(&self, _: WorkspaceSymbolParams) -> Self::SymbolFuture {
            Box::new(future::ok(None))
        }

        fn execute_command(&self, _: &Printer, _: ExecuteCommandParams) -> Self::ExecuteFuture {
            Box::new(future::ok(None))
        }

        fn completion(&self, _: CompletionParams) -> Self::CompletionFuture {
            Box::new(future::ok(None))
        }

        fn hover(&self, _: TextDocumentPositionParams) -> Self::HoverFuture {
            Box::new(future::ok(None))
        }

        fn document_highlight(&self, _: TextDocumentPositionParams) -> Self::HighlightFuture {
            Box::new(future::ok(None))
        }
    }

    #[test]
    fn exit_notification() {
        let (mut service, _) = LspService::new(Mock::default());

        let initialized: Incoming = r#"{"jsonrpc":"2.0","method":"initialized"}"#.parse().unwrap();
        assert_eq!(service.poll_ready(), Ok(Async::Ready(())));
        assert_eq!(service.call(initialized.clone()).wait(), Ok("".to_owned()));

        let exit: Incoming = r#"{"jsonrpc":"2.0","method":"exit"}"#.parse().unwrap();
        assert_eq!(service.poll_ready(), Ok(Async::Ready(())));
        assert_eq!(service.call(exit).wait(), Ok("".to_owned()));

        assert_eq!(service.poll_ready(), Ok(Async::NotReady));
        assert_eq!(service.call(initialized).wait(), Err(ExitedError));
    }
}
