//! Types for sending data to and from the language client.

pub use self::socket::{ClientSocket, RequestStream, ResponseSink};

use std::fmt::{self, Debug, Display, Formatter};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::channel::mpsc::{self, Sender};
use futures::future::BoxFuture;
use futures::sink::SinkExt;
use lsp_types::notification::*;
use lsp_types::request::*;
use lsp_types::*;
use serde::Serialize;
use serde_json::Value;
use tower::Service;
use tracing::{error, trace};

use self::pending::Pending;
use super::state::{ServerState, State};
use super::ExitedError;
use crate::jsonrpc::{self, Error, ErrorCode, Id, Request, Response};

mod pending;
mod socket;

struct ClientInner {
    tx: Sender<Request>,
    request_id: AtomicU32,
    pending: Arc<Pending>,
    state: Arc<ServerState>,
}

/// Handle for communicating with the language client.
///
/// This type provides a very cheap implementation of [`Clone`] so API consumers can cheaply clone
/// and pass it around as needed.
///
/// It also implements [`tower::Service`] in order to remain independent from the underlying
/// transport and to facilitate further abstraction with middleware.
#[derive(Clone)]
pub struct Client {
    inner: Arc<ClientInner>,
}

impl Client {
    pub(super) fn new(state: Arc<ServerState>) -> (Self, ClientSocket) {
        let (tx, rx) = mpsc::channel(1);
        let pending = Arc::new(Pending::new());

        let client = Client {
            inner: Arc::new(ClientInner {
                tx,
                request_id: AtomicU32::new(0),
                pending: pending.clone(),
                state: state.clone(),
            }),
        };

        (client, ClientSocket { rx, pending, state })
    }

    /// Disconnects the `Client` from its corresponding `LspService`.
    ///
    /// Closing the client is not required, but doing so will ensure that no more messages can be
    /// produced. The receiver of the messages will be able to consume any in-flight messages and
    /// then will observe the end of the stream.
    ///
    /// If the client is never closed and never dropped, the receiver of the messages will never
    /// observe the end of the stream.
    pub(crate) fn close(&self) {
        self.inner.tx.clone().close_channel();
    }
}

impl Client {
    // Lifecycle Messages

    /// Registers a new capability with the client.
    ///
    /// This corresponds to the [`client/registerCapability`] request.
    ///
    /// [`client/registerCapability`]: https://microsoft.github.io/language-server-protocol/specification#client_registerCapability
    ///
    /// # Initialization
    ///
    /// If the request is sent to the client before the server has been initialized, this will
    /// immediately return `Err` with JSON-RPC error code `-32002` ([read more]).
    ///
    /// [read more]: https://microsoft.github.io/language-server-protocol/specification#initialize
    pub async fn register_capability(
        &self,
        registrations: Vec<Registration>,
    ) -> jsonrpc::Result<()> {
        self.send_request::<RegisterCapability>(RegistrationParams { registrations })
            .await
    }

    /// Unregisters a capability with the client.
    ///
    /// This corresponds to the [`client/unregisterCapability`] request.
    ///
    /// [`client/unregisterCapability`]: https://microsoft.github.io/language-server-protocol/specification#client_unregisterCapability
    ///
    /// # Initialization
    ///
    /// If the request is sent to the client before the server has been initialized, this will
    /// immediately return `Err` with JSON-RPC error code `-32002` ([read more]).
    ///
    /// [read more]: https://microsoft.github.io/language-server-protocol/specification#initialize
    pub async fn unregister_capability(
        &self,
        unregisterations: Vec<Unregistration>,
    ) -> jsonrpc::Result<()> {
        self.send_request::<UnregisterCapability>(UnregistrationParams { unregisterations })
            .await
    }

    // Window Features

    /// Notifies the client to display a particular message in the user interface.
    ///
    /// This corresponds to the [`window/showMessage`] notification.
    ///
    /// [`window/showMessage`]: https://microsoft.github.io/language-server-protocol/specification#window_showMessage
    pub async fn show_message<M: Display>(&self, typ: MessageType, message: M) {
        self.send_notification_unchecked::<ShowMessage>(ShowMessageParams {
            typ,
            message: message.to_string(),
        })
        .await;
    }

    /// Requests the client to display a particular message in the user interface.
    ///
    /// Unlike the `show_message` notification, this request can also pass a list of actions and
    /// wait for an answer from the client.
    ///
    /// This corresponds to the [`window/showMessageRequest`] request.
    ///
    /// [`window/showMessageRequest`]: https://microsoft.github.io/language-server-protocol/specification#window_showMessageRequest
    pub async fn show_message_request<M: Display>(
        &self,
        typ: MessageType,
        message: M,
        actions: Option<Vec<MessageActionItem>>,
    ) -> jsonrpc::Result<Option<MessageActionItem>> {
        self.send_request_unchecked::<ShowMessageRequest>(ShowMessageRequestParams {
            typ,
            message: message.to_string(),
            actions,
        })
        .await
    }

    /// Notifies the client to log a particular message.
    ///
    /// This corresponds to the [`window/logMessage`] notification.
    ///
    /// [`window/logMessage`]: https://microsoft.github.io/language-server-protocol/specification#window_logMessage
    pub async fn log_message<M: Display>(&self, typ: MessageType, message: M) {
        self.send_notification_unchecked::<LogMessage>(LogMessageParams {
            typ,
            message: message.to_string(),
        })
        .await;
    }

    /// Asks the client to display a particular resource referenced by a URI in the user interface.
    ///
    /// Returns `Ok(true)` if the document was successfully shown, or `Ok(false)` otherwise.
    ///
    /// This corresponds to the [`window/showDocument`] request.
    ///
    /// [`window/showDocument`]: https://microsoft.github.io/language-server-protocol/specification#window_showDocument
    ///
    /// # Initialization
    ///
    /// If the request is sent to the client before the server has been initialized, this will
    /// immediately return `Err` with JSON-RPC error code `-32002` ([read more]).
    ///
    /// [read more]: https://microsoft.github.io/language-server-protocol/specification#initialize
    ///
    /// # Compatibility
    ///
    /// This request was introduced in specification version 3.16.0.
    pub async fn show_document(&self, params: ShowDocumentParams) -> jsonrpc::Result<bool> {
        let response = self.send_request::<ShowDocument>(params).await?;
        Ok(response.success)
    }

    // TODO: Add `work_done_progress_create()` here (since 3.15.0) when supported by `tower-lsp`.
    // https://github.com/ebkalderon/tower-lsp/issues/176

    /// Notifies the client to log a telemetry event.
    ///
    /// This corresponds to the [`telemetry/event`] notification.
    ///
    /// [`telemetry/event`]: https://microsoft.github.io/language-server-protocol/specification#telemetry_event
    pub async fn telemetry_event<S: Serialize>(&self, data: S) {
        match serde_json::to_value(data) {
            Err(e) => error!("invalid JSON in `telemetry/event` notification: {}", e),
            Ok(mut value) => {
                if !value.is_null() && !value.is_array() && !value.is_object() {
                    value = Value::Array(vec![value]);
                }
                self.send_notification_unchecked::<TelemetryEvent>(value)
                    .await;
            }
        }
    }

    /// Asks the client to refresh the code lenses currently shown in editors. As a result, the
    /// client should ask the server to recompute the code lenses for these editors.
    ///
    /// This is useful if a server detects a configuration change which requires a re-calculation
    /// of all code lenses.
    ///
    /// Note that the client still has the freedom to delay the re-calculation of the code lenses
    /// if for example an editor is currently not visible.
    ///
    /// This corresponds to the [`workspace/codeLens/refresh`] request.
    ///
    /// [`workspace/codeLens/refresh`]: https://microsoft.github.io/language-server-protocol/specification#codeLens_refresh
    ///
    /// # Initialization
    ///
    /// If the request is sent to the client before the server has been initialized, this will
    /// immediately return `Err` with JSON-RPC error code `-32002` ([read more]).
    ///
    /// [read more]: https://microsoft.github.io/language-server-protocol/specification#initialize
    ///
    /// # Compatibility
    ///
    /// This request was introduced in specification version 3.16.0.
    pub async fn code_lens_refresh(&self) -> jsonrpc::Result<()> {
        self.send_request::<CodeLensRefresh>(()).await
    }

    /// Asks the client to refresh the editors for which this server provides semantic tokens. As a
    /// result, the client should ask the server to recompute the semantic tokens for these
    /// editors.
    ///
    /// This is useful if a server detects a project-wide configuration change which requires a
    /// re-calculation of all semantic tokens. Note that the client still has the freedom to delay
    /// the re-calculation of the semantic tokens if for example an editor is currently not visible.
    ///
    /// This corresponds to the [`workspace/semanticTokens/refresh`] request.
    ///
    /// [`workspace/semanticTokens/refresh`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_semanticTokens
    ///
    /// # Initialization
    ///
    /// If the request is sent to the client before the server has been initialized, this will
    /// immediately return `Err` with JSON-RPC error code `-32002` ([read more]).
    ///
    /// [read more]: https://microsoft.github.io/language-server-protocol/specification#initialize
    ///
    /// # Compatibility
    ///
    /// This request was introduced in specification version 3.16.0.
    pub async fn semantic_tokens_refresh(&self) -> jsonrpc::Result<()> {
        self.send_request::<SemanticTokensRefresh>(()).await
    }

    /// Asks the client to refresh the inline values currently shown in editors. As a result, the
    /// client should ask the server to recompute the inline values for these editors.
    ///
    /// This is useful if a server detects a configuration change which requires a re-calculation
    /// of all inline values. Note that the client still has the freedom to delay the
    /// re-calculation of the inline values if for example an editor is currently not visible.
    ///
    /// This corresponds to the [`workspace/inlineValue/refresh`] request.
    ///
    /// [`workspace/inlineValue/refresh`]: https://microsoft.github.io/language-server-protocol/specification#workspace_inlineValue_refresh
    ///
    /// # Initialization
    ///
    /// If the request is sent to the client before the server has been initialized, this will
    /// immediately return `Err` with JSON-RPC error code `-32002` ([read more]).
    ///
    /// [read more]: https://microsoft.github.io/language-server-protocol/specification#initialize
    ///
    /// # Compatibility
    ///
    /// This request was introduced in specification version 3.17.0.
    pub async fn inline_value_refresh(&self) -> jsonrpc::Result<()> {
        self.send_request::<InlineValueRefreshRequest>(()).await
    }

    /// Asks the client to refresh the inlay hints currently shown in editors. As a result, the
    /// client should ask the server to recompute the inlay hints for these editors.
    ///
    /// This is useful if a server detects a configuration change which requires a re-calculation
    /// of all inlay hints. Note that the client still has the freedom to delay the re-calculation
    /// of the inlay hints if for example an editor is currently not visible.
    ///
    /// This corresponds to the [`workspace/inlayHint/refresh`] request.
    ///
    /// [`workspace/inlayHint/refresh`]: https://microsoft.github.io/language-server-protocol/specification#workspace_inlayHint_refresh
    ///
    /// # Initialization
    ///
    /// If the request is sent to the client before the server has been initialized, this will
    /// immediately return `Err` with JSON-RPC error code `-32002` ([read more]).
    ///
    /// [read more]: https://microsoft.github.io/language-server-protocol/specification#initialize
    ///
    /// # Compatibility
    ///
    /// This request was introduced in specification version 3.17.0.
    pub async fn inlay_hint_refresh(&self) -> jsonrpc::Result<()> {
        self.send_request::<InlayHintRefreshRequest>(()).await
    }

    /// Asks the client to refresh all needed document and workspace diagnostics.
    ///
    /// This is useful if a server detects a project wide configuration change which requires a
    /// re-calculation of all diagnostics.
    ///
    /// This corresponds to the [`workspace/diagnostic/refresh`] request.
    ///
    /// [`workspace/diagnostic/refresh`]: https://microsoft.github.io/language-server-protocol/specification#diagnostic_refresh
    ///
    /// # Initialization
    ///
    /// If the request is sent to the client before the server has been initialized, this will
    /// immediately return `Err` with JSON-RPC error code `-32002` ([read more]).
    ///
    /// [read more]: https://microsoft.github.io/language-server-protocol/specification#initialize
    ///
    /// # Compatibility
    ///
    /// This request was introduced in specification version 3.17.0.
    pub async fn workspace_diagnostic_refresh(&self) -> jsonrpc::Result<()> {
        self.send_request::<WorkspaceDiagnosticRefresh>(()).await
    }

    /// Submits validation diagnostics for an open file with the given URI.
    ///
    /// This corresponds to the [`textDocument/publishDiagnostics`] notification.
    ///
    /// [`textDocument/publishDiagnostics`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_publishDiagnostics
    ///
    /// # Initialization
    ///
    /// This notification will only be sent if the server is initialized.
    pub async fn publish_diagnostics(
        &self,
        uri: Url,
        diags: Vec<Diagnostic>,
        version: Option<i32>,
    ) {
        self.send_notification::<PublishDiagnostics>(PublishDiagnosticsParams::new(
            uri, diags, version,
        ))
        .await;
    }

    // Workspace Features

    /// Fetches configuration settings from the client.
    ///
    /// The request can fetch several configuration settings in one roundtrip. The order of the
    /// returned configuration settings correspond to the order of the passed
    /// [`ConfigurationItem`]s (e.g. the first item in the response is the result for the first
    /// configuration item in the params).
    ///
    /// This corresponds to the [`workspace/configuration`] request.
    ///
    /// [`workspace/configuration`]: https://microsoft.github.io/language-server-protocol/specification#workspace_configuration
    ///
    /// # Initialization
    ///
    /// If the request is sent to the client before the server has been initialized, this will
    /// immediately return `Err` with JSON-RPC error code `-32002` ([read more]).
    ///
    /// [read more]: https://microsoft.github.io/language-server-protocol/specification#initialize
    ///
    /// # Compatibility
    ///
    /// This request was introduced in specification version 3.6.0.
    pub async fn configuration(
        &self,
        items: Vec<ConfigurationItem>,
    ) -> jsonrpc::Result<Vec<Value>> {
        self.send_request::<WorkspaceConfiguration>(ConfigurationParams { items })
            .await
    }

    /// Fetches the current open list of workspace folders.
    ///
    /// Returns `None` if only a single file is open in the tool. Returns an empty `Vec` if a
    /// workspace is open but no folders are configured.
    ///
    /// This corresponds to the [`workspace/workspaceFolders`] request.
    ///
    /// [`workspace/workspaceFolders`]: https://microsoft.github.io/language-server-protocol/specification#workspace_workspaceFolders
    ///
    /// # Initialization
    ///
    /// If the request is sent to the client before the server has been initialized, this will
    /// immediately return `Err` with JSON-RPC error code `-32002` ([read more]).
    ///
    /// [read more]: https://microsoft.github.io/language-server-protocol/specification#initialize
    ///
    /// # Compatibility
    ///
    /// This request was introduced in specification version 3.6.0.
    pub async fn workspace_folders(&self) -> jsonrpc::Result<Option<Vec<WorkspaceFolder>>> {
        self.send_request::<WorkspaceFoldersRequest>(()).await
    }

    /// Requests a workspace resource be edited on the client side and returns whether the edit was
    /// applied.
    ///
    /// This corresponds to the [`workspace/applyEdit`] request.
    ///
    /// [`workspace/applyEdit`]: https://microsoft.github.io/language-server-protocol/specification#workspace_applyEdit
    ///
    /// # Initialization
    ///
    /// If the request is sent to the client before the server has been initialized, this will
    /// immediately return `Err` with JSON-RPC error code `-32002` ([read more]).
    ///
    /// [read more]: https://microsoft.github.io/language-server-protocol/specification#initialize
    pub async fn apply_edit(
        &self,
        edit: WorkspaceEdit,
    ) -> jsonrpc::Result<ApplyWorkspaceEditResponse> {
        self.send_request::<ApplyWorkspaceEdit>(ApplyWorkspaceEditParams { edit, label: None })
            .await
    }

    /// Sends a custom notification to the client.
    ///
    /// # Initialization
    ///
    /// This notification will only be sent if the server is initialized.
    pub async fn send_notification<N>(&self, params: N::Params)
    where
        N: lsp_types::notification::Notification,
    {
        if let State::Initialized | State::ShutDown = self.inner.state.get() {
            self.send_notification_unchecked::<N>(params).await;
        } else {
            let msg = Request::from_notification::<N>(params);
            trace!("server not initialized, supressing message: {}", msg);
        }
    }

    async fn send_notification_unchecked<N>(&self, params: N::Params)
    where
        N: lsp_types::notification::Notification,
    {
        let request = Request::from_notification::<N>(params);
        if self.clone().call(request).await.is_err() {
            error!("failed to send notification");
        }
    }

    /// Sends a custom request to the client.
    ///
    /// # Initialization
    ///
    /// If the request is sent to the client before the server has been initialized, this will
    /// immediately return `Err` with JSON-RPC error code `-32002` ([read more]).
    ///
    /// [read more]: https://microsoft.github.io/language-server-protocol/specification#initialize
    pub async fn send_request<R>(&self, params: R::Params) -> jsonrpc::Result<R::Result>
    where
        R: lsp_types::request::Request,
    {
        if let State::Initialized | State::ShutDown = self.inner.state.get() {
            self.send_request_unchecked::<R>(params).await
        } else {
            let id = self.inner.request_id.load(Ordering::SeqCst) as i64 + 1;
            let msg = Request::from_request::<R>(id.into(), params);
            trace!("server not initialized, supressing message: {}", msg);
            Err(jsonrpc::not_initialized_error())
        }
    }

    async fn send_request_unchecked<R>(&self, params: R::Params) -> jsonrpc::Result<R::Result>
    where
        R: lsp_types::request::Request,
    {
        let id = self.next_request_id();
        let request = Request::from_request::<R>(id, params);

        let response = match self.clone().call(request).await {
            Ok(Some(response)) => response,
            Ok(None) | Err(_) => return Err(Error::internal_error()),
        };

        let (_, result) = response.into_parts();
        result.and_then(|v| {
            serde_json::from_value(v).map_err(|e| Error {
                code: ErrorCode::ParseError,
                message: e.to_string().into(),
                data: None,
            })
        })
    }
}

impl Client {
    /// Increments the internal request ID counter and returns the previous value.
    ///
    /// This method can be used to build custom [`Request`] objects with numeric IDs that are
    /// guaranteed to be unique every time.
    pub fn next_request_id(&self) -> Id {
        let num = self.inner.request_id.fetch_add(1, Ordering::Relaxed);
        Id::Number(num as i64)
    }
}

impl Debug for Client {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("Client")
            .field("tx", &self.inner.tx)
            .field("pending", &self.inner.pending)
            .field("request_id", &self.inner.request_id)
            .field("state", &self.inner.state)
            .finish()
    }
}

impl Service<Request> for Client {
    type Response = Option<Response>;
    type Error = ExitedError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner
            .tx
            .clone()
            .poll_ready(cx)
            .map_err(|_| ExitedError(()))
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let mut tx = self.inner.tx.clone();
        let response_waiter = req.id().cloned().map(|id| self.inner.pending.wait(id));

        Box::pin(async move {
            if tx.send(req).await.is_err() {
                return Err(ExitedError(()));
            }

            match response_waiter {
                Some(fut) => Ok(Some(fut.await)),
                None => Ok(None),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;

    use futures::stream::StreamExt;
    use serde_json::json;

    use super::*;

    async fn assert_client_message<F, Fut>(f: F, expected: Request)
    where
        F: FnOnce(Client) -> Fut,
        Fut: Future,
    {
        let state = Arc::new(ServerState::new());
        state.set(State::Initialized);

        let (client, socket) = Client::new(state);
        f(client).await;

        let messages: Vec<_> = socket.collect().await;
        assert_eq!(messages, vec![expected]);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn log_message() {
        let (typ, msg) = (MessageType::LOG, "foo bar".to_owned());
        let expected = Request::from_notification::<LogMessage>(LogMessageParams {
            typ,
            message: msg.clone(),
        });

        assert_client_message(|p| async move { p.log_message(typ, msg).await }, expected).await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn show_message() {
        let (typ, msg) = (MessageType::LOG, "foo bar".to_owned());
        let expected = Request::from_notification::<ShowMessage>(ShowMessageParams {
            typ,
            message: msg.clone(),
        });

        assert_client_message(|p| async move { p.show_message(typ, msg).await }, expected).await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn telemetry_event() {
        let null = json!(null);
        let expected = Request::from_notification::<TelemetryEvent>(null.clone());
        assert_client_message(|p| async move { p.telemetry_event(null).await }, expected).await;

        let array = json!([1, 2, 3]);
        let expected = Request::from_notification::<TelemetryEvent>(array.clone());
        assert_client_message(|p| async move { p.telemetry_event(array).await }, expected).await;

        let object = json!({});
        let expected = Request::from_notification::<TelemetryEvent>(object.clone());
        assert_client_message(|p| async move { p.telemetry_event(object).await }, expected).await;

        let other = json!("hello");
        let wrapped = Value::Array(vec![other.clone()]);
        let expected = Request::from_notification::<TelemetryEvent>(wrapped);
        assert_client_message(|p| async move { p.telemetry_event(other).await }, expected).await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn publish_diagnostics() {
        let uri: Url = "file:///path/to/file".parse().unwrap();
        let diagnostics = vec![Diagnostic::new_simple(Default::default(), "example".into())];

        let params = PublishDiagnosticsParams::new(uri.clone(), diagnostics.clone(), None);
        let expected = Request::from_notification::<PublishDiagnostics>(params);

        assert_client_message(
            |p| async move { p.publish_diagnostics(uri, diagnostics, None).await },
            expected,
        )
        .await;
    }
}
