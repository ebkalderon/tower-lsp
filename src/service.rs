//! Service abstraction for language servers.

pub(crate) use self::state::{ServerState, State};

use std::fmt::{self, Display, Formatter};
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::future::{self, BoxFuture, FutureExt};
use serde_json::Value;
use tower::Service;

use crate::jsonrpc::{Error, ErrorCode, Request, Response, Router};
use crate::LanguageServer;

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
