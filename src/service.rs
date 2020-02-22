//! Service abstraction for language servers.

use std::error::Error;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use futures::channel::oneshot::{self, Canceled};
use futures::compat::Future01CompatExt;
use futures::future::{FutureExt, Shared, TryFutureExt};
use futures::{future, select};
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
    pub async fn run_until_exit<F>(self, future: F)
    where
        F: Future<Output = ()> + Send,
    {
        select! {
            a = self.0.fuse() => (),
            b = future.fuse() => (),
        }
    }
}

impl Future for ExitReceiver {
    type Output = Result<(), Canceled>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let recv = &mut self.as_mut().0;
        Pin::new(recv).poll(cx)
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
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context) -> Poll<Result<(), Self::Error>> {
        if self.stopped.load(Ordering::SeqCst) {
            Poll::Pending
        } else {
            Poll::Ready(Ok(()))
        }
    }

    fn call(&mut self, request: Incoming) -> Self::Future {
        if self.stopped.load(Ordering::SeqCst) {
            Box::pin(future::err(ExitedError))
        } else {
            if let Incoming::Response(r) = request {
                // FIXME: Currently, we are dropping responses to requests created in `Printer`.
                // We need some way to route them back to the `Printer`. See this issue for more:
                //
                // https://github.com/ebkalderon/tower-lsp/issues/13
                debug!("dropping client response, as per GitHub issue #13: {:?}", r);
                Box::pin(future::ok(String::new()))
            } else {
                Box::pin(
                    self.handler
                        .handle_request(&request.to_string())
                        .compat()
                        .map_err(|_| unreachable!())
                        .map_ok(move |result| {
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
    use async_trait::async_trait;
    use jsonrpc_core::Result;
    use lsp_types::request::{GotoDefinitionResponse, GotoImplementationResponse};
    use lsp_types::*;
    use serde_json::Value;
    use tower_test::mock::Spawn;

    use super::*;
    use crate::Printer;

    #[derive(Debug, Default)]
    struct Mock;

    #[async_trait]
    impl LanguageServer for Mock {
        fn initialize(&self, _: &Printer, _: InitializeParams) -> Result<InitializeResult> {
            Ok(InitializeResult::default())
        }

        async fn shutdown(&self) -> Result<()> {
            Ok(())
        }

        async fn symbol(&self, _: WorkspaceSymbolParams) -> Result<Option<Vec<SymbolInformation>>> {
            Ok(None)
        }

        async fn execute_command(
            &self,
            _: &Printer,
            _: ExecuteCommandParams,
        ) -> Result<Option<Value>> {
            Ok(None)
        }

        async fn completion(&self, _: CompletionParams) -> Result<Option<CompletionResponse>> {
            Ok(None)
        }

        async fn hover(&self, _: TextDocumentPositionParams) -> Result<Option<Hover>> {
            Ok(None)
        }

        async fn signature_help(
            &self,
            _: TextDocumentPositionParams,
        ) -> Result<Option<SignatureHelp>> {
            Ok(None)
        }

        async fn goto_declaration(
            &self,
            _: TextDocumentPositionParams,
        ) -> Result<Option<GotoDefinitionResponse>> {
            Ok(None)
        }

        async fn goto_definition(
            &self,
            _: TextDocumentPositionParams,
        ) -> Result<Option<GotoDefinitionResponse>> {
            Ok(None)
        }

        async fn goto_type_definition(
            &self,
            _: TextDocumentPositionParams,
        ) -> Result<Option<GotoDefinitionResponse>> {
            Ok(None)
        }

        async fn goto_implementation(
            &self,
            _: TextDocumentPositionParams,
        ) -> Result<Option<GotoImplementationResponse>> {
            Ok(None)
        }

        async fn document_highlight(
            &self,
            _: TextDocumentPositionParams,
        ) -> Result<Option<Vec<DocumentHighlight>>> {
            Ok(None)
        }
    }

    #[tokio::test]
    async fn exit_notification() {
        let (service, _) = LspService::new(Mock::default());
        let mut service = Spawn::new(service);

        let initialized: Incoming = r#"{"jsonrpc":"2.0","method":"initialized"}"#.parse().unwrap();
        assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
        assert_eq!(service.call(initialized.clone()).await, Ok("".to_owned()));

        let exit: Incoming = r#"{"jsonrpc":"2.0","method":"exit"}"#.parse().unwrap();
        assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
        assert_eq!(service.call(exit).await, Ok("".to_owned()));

        assert_eq!(service.poll_ready(), Poll::Pending);
        assert_eq!(service.call(initialized).await, Err(ExitedError));
    }
}
