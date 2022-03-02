//! Service abstraction for language servers.

pub use self::client::{Client, ClientSocket, RequestStream, ResponseSink};

pub(crate) use self::pending::Pending;
pub(crate) use self::state::{ServerState, State};

use std::fmt::{self, Display, Formatter};
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::future::{self, BoxFuture, FutureExt};
use serde_json::Value;
use tower::Service;

use crate::jsonrpc::{Error, ErrorCode, Request, Response, Router};
use crate::LanguageServer;

pub(crate) mod layers;

mod client;
mod pending;
mod state;

/// Error that occurs when attempting to call the language server after it has already exited.
#[derive(Clone, Debug, PartialEq)]
pub struct ExitedError(());

impl std::error::Error for ExitedError {}

impl Display for ExitedError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str("language server has exited")
    }
}

/// Service abstraction for the Language Server Protocol.
///
/// This service takes an incoming JSON-RPC message as input and produces an outgoing message as
/// output. If the incoming message is a server notification or a client response, then the
/// corresponding response will be `None`.
///
/// This implements [`tower::Service`] in order to remain independent from the underlying transport
/// and to facilitate further abstraction with middleware.
///
/// Pending requests can be canceled by issuing a [`$/cancelRequest`] notification.
///
/// [`$/cancelRequest`]: https://microsoft.github.io/language-server-protocol/specification#cancelRequest
///
/// The service shuts down and stops serving requests after the [`exit`] notification is received.
///
/// [`exit`]: https://microsoft.github.io/language-server-protocol/specification#exit
#[derive(Debug)]
pub struct LspService<S> {
    inner: Router<S, ExitedError>,
    state: Arc<ServerState>,
}

impl<S: LanguageServer> LspService<S> {
    /// Creates a new `LspService` with the given server backend, also returning a channel for
    /// server-to-client communication.
    pub fn new<F>(init: F) -> (Self, ClientSocket)
    where
        F: FnOnce(Client) -> S,
    {
        let state = Arc::new(ServerState::new());

        let (client, socket) = Client::new(state.clone());
        let inner = Router::new(init(client.clone()));
        let pending = Arc::new(Pending::new());

        let service = LspService {
            inner: crate::generated::register_lsp_methods(inner, state.clone(), pending, client),
            state,
        };

        (service, socket)
    }
}

impl<S: LanguageServer> Service<Request> for LspService<S> {
    type Response = Option<Response>;
    type Error = ExitedError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.state.get() {
            State::Initializing => Poll::Pending,
            State::Exited => Poll::Ready(Err(ExitedError(()))),
            _ => self.inner.poll_ready(cx),
        }
    }

    fn call(&mut self, req: Request) -> Self::Future {
        if self.state.get() == State::Exited {
            return future::err(ExitedError(())).boxed();
        }

        let fut = self.inner.call(req);

        Box::pin(async move {
            let response = fut.await?;

            match response.as_ref().and_then(|res| res.error()) {
                Some(Error {
                    code: ErrorCode::MethodNotFound,
                    data: Some(Value::String(m)),
                    ..
                }) if m.starts_with("$/") => Ok(None),
                _ => Ok(response),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use lsp_types::*;
    use serde_json::json;
    use tower::ServiceExt;

    use super::*;
    use crate::jsonrpc::Result;

    #[derive(Debug)]
    struct Mock;

    #[async_trait]
    impl LanguageServer for Mock {
        async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
            Ok(InitializeResult::default())
        }

        async fn shutdown(&self) -> Result<()> {
            Ok(())
        }

        // This handler should never resolve...
        async fn code_action_resolve(&self, _: CodeAction) -> Result<CodeAction> {
            future::pending().await
        }
    }

    fn initialize_request(id: i64) -> Request {
        Request::build("initialize")
            .params(json!({"capabilities":{}}))
            .id(id)
            .finish()
    }

    #[tokio::test(flavor = "current_thread")]
    async fn initializes_only_once() {
        let (mut service, _) = LspService::new(|_| Mock);

        let request = initialize_request(1);

        let response = service.ready().await.unwrap().call(request.clone()).await;
        let ok = Response::from_ok(1.into(), json!({"capabilities":{}}));
        assert_eq!(response, Ok(Some(ok)));

        let response = service.ready().await.unwrap().call(request).await;
        let err = Response::from_error(1.into(), Error::invalid_request());
        assert_eq!(response, Ok(Some(err)));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn refuses_requests_after_shutdown() {
        let (mut service, _) = LspService::new(|_| Mock);

        let initialize = initialize_request(1);
        let response = service.ready().await.unwrap().call(initialize).await;
        let ok = Response::from_ok(1.into(), json!({"capabilities":{}}));
        assert_eq!(response, Ok(Some(ok)));

        let shutdown = Request::build("shutdown").id(1).finish();
        let response = service.ready().await.unwrap().call(shutdown.clone()).await;
        let ok = Response::from_ok(1.into(), json!(null));
        assert_eq!(response, Ok(Some(ok)));

        let response = service.ready().await.unwrap().call(shutdown).await;
        let err = Response::from_error(1.into(), Error::invalid_request());
        assert_eq!(response, Ok(Some(err)));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn exit_notification() {
        let (mut service, _) = LspService::new(|_| Mock);

        let exit = Request::build("exit").finish();
        let response = service.ready().await.unwrap().call(exit.clone()).await;
        assert_eq!(response, Ok(None));

        let ready = future::poll_fn(|cx| service.poll_ready(cx)).await;
        assert_eq!(ready, Err(ExitedError(())));
        assert_eq!(service.call(exit).await, Err(ExitedError(())));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancels_pending_requests() {
        let (mut service, _) = LspService::new(|_| Mock);

        let initialize = initialize_request(1);
        let response = service.ready().await.unwrap().call(initialize).await;
        let ok = Response::from_ok(1.into(), json!({"capabilities":{}}));
        assert_eq!(response, Ok(Some(ok)));

        let pending_request = Request::build("codeAction/resolve")
            .params(json!({"title":""}))
            .id(1)
            .finish();

        let cancel_request = Request::build("$/cancelRequest")
            .params(json!({"id":1i32}))
            .finish();

        let pending_fut = service.ready().await.unwrap().call(pending_request);
        let cancel_fut = service.ready().await.unwrap().call(cancel_request);
        let (pending_response, cancel_response) = futures::join!(pending_fut, cancel_fut);

        let canceled = Response::from_error(1.into(), Error::request_cancelled());
        assert_eq!(pending_response, Ok(Some(canceled)));
        assert_eq!(cancel_response, Ok(None));
    }
}
