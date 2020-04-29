//! Service abstraction for language servers.

use std::error::Error;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::compat::Future01CompatExt;
use futures::future::{self, TryFutureExt};
use futures::sink::SinkExt;
use jsonrpc_core::IoHandler;
use log::info;
use lsp_types::notification::{Exit, Notification};
use tower_service::Service;

use super::delegate::{Delegate, LanguageServerCore, MessageSender, MessageStream};
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

/// Service abstraction for the Language Server Protocol.
///
/// This service takes a JSON-RPC request as input and produces a JSON-RPC response as output. If
/// the incoming request is a notification, then the corresponding response string will be empty.
///
/// This implements [`tower_service::Service`] in order to remain independent from the underlying
/// transport and to facilitate further abstraction with middleware.
///
/// [`tower_service::Service`]: https://docs.rs/tower-service/0.3.0/tower_service/trait.Service.html
///
/// The service shuts down and stops serving requests after the [`exit`] notification is received.
///
/// [`exit`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#exit
#[derive(Debug)]
pub struct LspService {
    handler: IoHandler,
    sender: MessageSender,
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
        let (delegate, messages, sender) = Delegate::new(server);

        let mut handler = handler.into();
        handler.extend_with(delegate.to_delegate());

        let stopped = Arc::new(AtomicBool::new(false));
        let stopped_arc = stopped.clone();
        handler.add_notification(Exit::METHOD, move |_| {
            info!("exit notification received, shutting down");
            stopped_arc.store(true, Ordering::SeqCst);
        });

        let service = LspService {
            handler,
            stopped,
            sender,
        };

        (service, messages)
    }
}

impl Service<Incoming> for LspService {
    type Response = Option<String>;
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
            Box::pin(future::err(ExitedError))
        } else {
            if let Incoming::Response(res) = request {
                let mut sender = self.sender.clone();
                Box::pin(async move {
                    sender.send(res).await.expect("LspService already dropped");
                    Ok(None)
                })
            } else {
                Box::pin(
                    self.handler
                        .handle_request(&request.to_string())
                        .compat()
                        .map_err(|_| unreachable!()),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use jsonrpc_core::Result;
    use lsp_types::*;
    use tower_test::mock::Spawn;

    use super::*;
    use crate::Client;

    const INITIALIZE_REQUEST: &str =
        r#"{"jsonrpc":"2.0","method":"initialize","params":{"capabilities":{}},"id":1}"#;

    #[derive(Debug, Default)]
    struct Mock;

    #[async_trait]
    impl LanguageServer for Mock {
        async fn initialize(&self, _: &Client, _: InitializeParams) -> Result<InitializeResult> {
            Ok(InitializeResult::default())
        }

        async fn shutdown(&self) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn initializes_only_once() {
        let (service, _) = LspService::new(Mock::default());
        let mut service = Spawn::new(service);

        let initialize: Incoming = INITIALIZE_REQUEST.parse().unwrap();
        let ok = r#"{"jsonrpc":"2.0","result":{"capabilities":{}},"id":1}"#;
        assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
        assert_eq!(service.call(initialize.clone()).await, Ok(Some(ok.into())));

        let err = r#"{"jsonrpc":"2.0","error":{"code":-32600,"message":"Invalid request"},"id":1}"#;
        assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
        assert_eq!(service.call(initialize).await, Ok(Some(err.into())));
    }

    #[tokio::test]
    async fn exit_notification() {
        let (service, _) = LspService::new(Mock::default());
        let mut service = Spawn::new(service);

        let initialized: Incoming = r#"{"jsonrpc":"2.0","method":"initialized"}"#.parse().unwrap();
        assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
        assert_eq!(service.call(initialized.clone()).await, Ok(None));

        let exit: Incoming = r#"{"jsonrpc":"2.0","method":"exit"}"#.parse().unwrap();
        assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
        assert_eq!(service.call(exit).await, Ok(None));

        assert_eq!(service.poll_ready(), Poll::Ready(Err(ExitedError)));
        assert_eq!(service.call(initialized).await, Err(ExitedError));
    }
}
