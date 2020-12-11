//! Types for sending data to and from the language client.

use std::fmt::{self, Debug, Display, Formatter};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use futures::channel::mpsc::Sender;
use futures::sink::SinkExt;
use log::{error, trace};
use lsp_types::notification::{Notification, *};
use lsp_types::request::{Request, *};
use lsp_types::*;
use serde::Serialize;
use serde_json::Value;

use super::jsonrpc::{self, ClientRequest, ClientRequests, Error, ErrorCode, Id, Outgoing, Result};
use super::{ServerState, State};

struct ClientInner {
    sender: Sender<Outgoing>,
    request_id: AtomicU64,
    pending_requests: Arc<ClientRequests>,
    state: Arc<ServerState>,
}

/// Handle for communicating with the language client.
///
/// This type provides a very cheap implementation of [`Clone`] so API consumers can cheaply clone
/// and pass it around as needed.
///
/// [`Clone`]: trait@std::clone::Clone
#[derive(Clone)]
pub struct Client {
    inner: Arc<ClientInner>,
}

impl Client {
    pub(super) fn new(
        sender: Sender<Outgoing>,
        pending_requests: Arc<ClientRequests>,
        state: Arc<ServerState>,
    ) -> Self {
        Client {
            inner: Arc::new(ClientInner {
                sender,
                request_id: AtomicU64::new(0),
                pending_requests,
                state,
            }),
        }
    }

    /// Notifies the client to log a particular message.
    ///
    /// This corresponds to the [`window/logMessage`] notification.
    ///
    /// [`window/logMessage`]: https://microsoft.github.io/language-server-protocol/specification#window_logMessage
    pub async fn log_message<M: Display>(&self, typ: MessageType, message: M) {
        self.send_notification::<LogMessage>(LogMessageParams {
            typ,
            message: message.to_string(),
        })
        .await;
    }

    /// Notifies the client to display a particular message in the user interface.
    ///
    /// This corresponds to the [`window/showMessage`] notification.
    ///
    /// [`window/showMessage`]: https://microsoft.github.io/language-server-protocol/specification#window_showMessage
    pub async fn show_message<M: Display>(&self, typ: MessageType, message: M) {
        self.send_notification::<ShowMessage>(ShowMessageParams {
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
    ) -> Result<Option<MessageActionItem>> {
        self.send_request::<ShowMessageRequest>(ShowMessageRequestParams {
            typ,
            message: message.to_string(),
            actions,
        })
        .await
    }

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
                self.send_notification::<TelemetryEvent>(value).await;
            }
        }
    }

    /// Registers a new capability with the client.
    ///
    /// This corresponds to the [`client/registerCapability`] request.
    ///
    /// [`client/registerCapability`]: https://microsoft.github.io/language-server-protocol/specification#client_registerCapability
    ///
    /// # Initialization
    ///
    /// If the request is sent to client before the server has been initialized, this will
    /// immediately return `Err` with JSON-RPC error code `-32002` ([read more]).
    ///
    /// [read more]: https://microsoft.github.io/language-server-protocol/specification#initialize
    pub async fn register_capability(&self, registrations: Vec<Registration>) -> Result<()> {
        self.send_request_initialized::<RegisterCapability>(RegistrationParams { registrations })
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
    /// If the request is sent to client before the server has been initialized, this will
    /// immediately return `Err` with JSON-RPC error code `-32002` ([read more]).
    ///
    /// [read more]: https://microsoft.github.io/language-server-protocol/specification#initialize
    pub async fn unregister_capability(&self, unregisterations: Vec<Unregistration>) -> Result<()> {
        self.send_request_initialized::<UnregisterCapability>(UnregistrationParams {
            unregisterations,
        })
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
    /// If the request is sent to client before the server has been initialized, this will
    /// immediately return `Err` with JSON-RPC error code `-32002` ([read more]).
    ///
    /// [read more]: https://microsoft.github.io/language-server-protocol/specification#initialize
    ///
    /// # Compatibility
    ///
    /// This request was introduced in specification version 3.6.0.
    pub async fn workspace_folders(&self) -> Result<Option<Vec<WorkspaceFolder>>> {
        self.send_request_initialized::<WorkspaceFoldersRequest>(())
            .await
    }

    /// Fetches configuration settings from the client.
    ///
    /// The request can fetch several configuration settings in one roundtrip. The order of the
    /// returned configuration settings correspond to the order of the passed
    /// [`ConfigurationItem`]s (e.g. the first item in the response is the result for the first
    /// configuration item in the params).
    ///
    /// [`ConfigurationItem`]: https://docs.rs/lsp-types/0.74.0/lsp_types/struct.ConfigurationItem.html
    ///
    /// This corresponds to the [`workspace/configuration`] request.
    ///
    /// [`workspace/configuration`]: https://microsoft.github.io/language-server-protocol/specification#workspace_configuration
    ///
    /// # Initialization
    ///
    /// If the request is sent to client before the server has been initialized, this will
    /// immediately return `Err` with JSON-RPC error code `-32002` ([read more]).
    ///
    /// [read more]: https://microsoft.github.io/language-server-protocol/specification#initialize
    ///
    /// # Compatibility
    ///
    /// This request was introduced in specification version 3.6.0.
    pub async fn configuration(&self, items: Vec<ConfigurationItem>) -> Result<Vec<Value>> {
        self.send_request_initialized::<WorkspaceConfiguration>(ConfigurationParams { items })
            .await
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
    /// If the request is sent to client before the server has been initialized, this will
    /// immediately return `Err` with JSON-RPC error code `-32002` ([read more]).
    ///
    /// [read more]: https://microsoft.github.io/language-server-protocol/specification#initialize
    pub async fn apply_edit(
        &self,
        edit: WorkspaceEdit,
        label: Option<String>,
    ) -> Result<ApplyWorkspaceEditResponse> {
        self.send_request_initialized::<ApplyWorkspaceEdit>(ApplyWorkspaceEditParams {
            edit,
            label,
        })
        .await
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
        self.send_notification_initialized::<PublishDiagnostics>(PublishDiagnosticsParams::new(
            uri, diags, version,
        ))
        .await;
    }

    /// Sends a custom notification to the client.
    ///
    /// # Initialization
    ///
    /// This notification will only be sent if the server is initialized.
    pub async fn send_custom_notification<N>(&self, params: N::Params)
    where
        N: Notification,
    {
        self.send_notification_initialized::<N>(params).await;
    }

    async fn send_notification<N>(&self, params: N::Params)
    where
        N: Notification,
    {
        let mut sender = self.inner.sender.clone();
        let message = Outgoing::Request(ClientRequest::notification::<N>(params));
        if sender.send(message).await.is_err() {
            error!("failed to send notification")
        }
    }

    async fn send_notification_initialized<N>(&self, params: N::Params)
    where
        N: Notification,
    {
        if let State::Initialized | State::ShutDown = self.inner.state.get() {
            self.send_notification::<N>(params).await;
        } else {
            let msg = ClientRequest::notification::<N>(params);
            trace!("server not initialized, supressing message: {}", msg);
        }
    }

    async fn send_request<R>(&self, params: R::Params) -> Result<R::Result>
    where
        R: Request,
    {
        let id = self.inner.request_id.fetch_add(1, Ordering::Relaxed);
        let message = Outgoing::Request(ClientRequest::request::<R>(id, params));

        if self.inner.sender.clone().send(message).await.is_err() {
            error!("failed to send request");
            return Err(Error::internal_error());
        }

        let response = self.inner.pending_requests.wait(Id::Number(id)).await;
        let (_, result) = response.into_parts();
        result.and_then(|v| {
            serde_json::from_value(v).map_err(|e| Error {
                code: ErrorCode::ParseError,
                message: e.to_string(),
                data: None,
            })
        })
    }

    async fn send_request_initialized<R>(&self, params: R::Params) -> Result<R::Result>
    where
        R: Request,
    {
        if let State::Initialized | State::ShutDown = self.inner.state.get() {
            self.send_request::<R>(params).await
        } else {
            let id = self.inner.request_id.load(Ordering::SeqCst) + 1;
            let msg = ClientRequest::request::<R>(id, params);
            trace!("server not initialized, supressing message: {}", msg);
            Err(jsonrpc::not_initialized_error())
        }
    }
}

impl Debug for Client {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct(stringify!(Client))
            .field("request_id", &self.inner.request_id)
            .field("pending_requests", &self.inner.pending_requests)
            .field("state", &self.inner.state)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;

    use futures::channel::mpsc;
    use futures::StreamExt;
    use serde_json::json;

    use super::*;

    async fn assert_client_messages<F, Fut>(f: F, expected: ClientRequest)
    where
        F: FnOnce(Client) -> Fut,
        Fut: Future,
    {
        let (request_tx, request_rx) = mpsc::channel(1);
        let pending = Arc::new(ClientRequests::new());
        let state = Arc::new(ServerState::new());
        state.set(State::Initialized);

        let client = Client::new(request_tx, pending, state);
        f(client).await;

        let messages: Vec<_> = request_rx.collect().await;
        assert_eq!(messages, vec![Outgoing::Request(expected)]);
    }

    #[tokio::test]
    async fn log_message() {
        let (typ, msg) = (MessageType::Log, "foo bar".to_owned());
        let expected = ClientRequest::notification::<LogMessage>(LogMessageParams {
            typ,
            message: msg.clone(),
        });

        assert_client_messages(|p| async move { p.log_message(typ, msg).await }, expected).await;
    }

    #[tokio::test]
    async fn show_message() {
        let (typ, msg) = (MessageType::Log, "foo bar".to_owned());
        let expected = ClientRequest::notification::<ShowMessage>(ShowMessageParams {
            typ,
            message: msg.clone(),
        });

        assert_client_messages(|p| async move { p.show_message(typ, msg).await }, expected).await;
    }

    #[tokio::test]
    async fn telemetry_event() {
        let null = json!(null);
        let expected = ClientRequest::notification::<TelemetryEvent>(null.clone());
        assert_client_messages(|p| async move { p.telemetry_event(null).await }, expected).await;

        let array = json!([1, 2, 3]);
        let expected = ClientRequest::notification::<TelemetryEvent>(array.clone());
        assert_client_messages(|p| async move { p.telemetry_event(array).await }, expected).await;

        let object = json!({});
        let expected = ClientRequest::notification::<TelemetryEvent>(object.clone());
        assert_client_messages(|p| async move { p.telemetry_event(object).await }, expected).await;

        let other = json!("hello");
        let wrapped = Value::Array(vec![other.clone()]);
        let expected = ClientRequest::notification::<TelemetryEvent>(wrapped);
        assert_client_messages(|p| async move { p.telemetry_event(other).await }, expected).await;
    }

    #[tokio::test]
    async fn publish_diagnostics() {
        let uri: Url = "file:///path/to/file".parse().unwrap();
        let diagnostics = vec![Diagnostic::new_simple(Default::default(), "example".into())];

        let params = PublishDiagnosticsParams::new(uri.clone(), diagnostics.clone(), None);
        let expected = ClientRequest::notification::<PublishDiagnostics>(params);

        assert_client_messages(
            |p| async move { p.publish_diagnostics(uri, diagnostics, None).await },
            expected,
        )
        .await;
    }
}
