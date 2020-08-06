//! Types for sending data to and from the language client.

use std::fmt::Display;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use futures::channel::mpsc::Sender;
use futures::sink::SinkExt;
use log::{error, trace};
use lsp_types::notification::{Notification, *};
use lsp_types::request::{Request, *};
use lsp_types::*;
use serde::Serialize;
use serde_json::Value;

use super::jsonrpc::{self, ClientRequests, Error, ErrorCode, Id, Outgoing, Result, Version};

#[derive(Debug)]
struct ClientInner {
    sender: Sender<Outgoing>,
    initialized: Arc<AtomicBool>,
    request_id: AtomicU64,
    pending_requests: Arc<ClientRequests>,
}

/// Handle for communicating with the language client.
#[derive(Clone, Debug)]
pub struct Client {
    inner: Arc<ClientInner>,
}

impl Client {
    pub(super) fn new(
        sender: Sender<Outgoing>,
        pending_requests: Arc<ClientRequests>,
        initialized: Arc<AtomicBool>,
    ) -> Self {
        Client {
            inner: Arc::new(ClientInner {
                sender,
                initialized,
                request_id: AtomicU64::new(0),
                pending_requests,
            }),
        }
    }

    /// Notifies the client to log a particular message.
    ///
    /// This corresponds to the [`window/logMessage`] notification.
    ///
    /// [`window/logMessage`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#window_logMessage
    pub fn log_message<M: Display>(&self, typ: MessageType, message: M) {
        self.send_notification::<LogMessage>(LogMessageParams {
            typ,
            message: message.to_string(),
        });
    }

    /// Notifies the client to display a particular message in the user interface.
    ///
    /// This corresponds to the [`window/showMessage`] notification.
    ///
    /// [`window/showMessage`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#window_showMessage
    pub fn show_message<M: Display>(&self, typ: MessageType, message: M) {
        self.send_notification::<ShowMessage>(ShowMessageParams {
            typ,
            message: message.to_string(),
        });
    }

    /// Asks the client to display a particular message in the user interface.
    ///
    /// In addition to the `show_message` notification, the request allows to pass actions and to
    /// wait for an answer from the client.
    ///
    /// This corresponds to the [`window/showMessageRequest`] request.
    ///
    /// [`window/showMessageRequest`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#window_showMessageRequest
    ///
    /// # Initialization
    ///
    /// If the request is sent to client before the server has been initialized, this will
    /// immediately return `Err` with JSON-RPC error code `-32002` ([read more]).
    ///
    /// [read more]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#initialize
    pub async fn show_message_request<M: Display>(
        &self,
        typ: MessageType,
        message: M,
        actions: Option<Vec<MessageActionItem>>,
    ) -> Result<Option<MessageActionItem>> {
        self.send_request_initialized::<ShowMessageRequest>(ShowMessageRequestParams {
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
    /// [`telemetry/event`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#telemetry_event
    pub fn telemetry_event<S: Serialize>(&self, data: S) {
        match serde_json::to_value(data) {
            Err(e) => error!("invalid JSON in `telemetry/event` notification: {}", e),
            Ok(mut value) => {
                if !value.is_null() && !value.is_array() && !value.is_object() {
                    value = Value::Array(vec![value]);
                }
                self.send_notification::<TelemetryEvent>(value);
            }
        }
    }

    /// Registers a new capability with the client.
    ///
    /// This corresponds to the [`client/registerCapability`] request.
    ///
    /// [`client/registerCapability`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#client_registerCapability
    ///
    /// # Initialization
    ///
    /// If the request is sent to client before the server has been initialized, this will
    /// immediately return `Err` with JSON-RPC error code `-32002` ([read more]).
    ///
    /// [read more]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#initialize
    pub async fn register_capability(&self, registrations: Vec<Registration>) -> Result<()> {
        self.send_request_initialized::<RegisterCapability>(RegistrationParams { registrations })
            .await
    }

    /// Unregisters a capability with the client.
    ///
    /// This corresponds to the [`client/unregisterCapability`] request.
    ///
    /// [`client/unregisterCapability`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#client_unregisterCapability
    ///
    /// # Initialization
    ///
    /// If the request is sent to client before the server has been initialized, this will
    /// immediately return `Err` with JSON-RPC error code `-32002` ([read more]).
    ///
    /// [read more]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#initialize
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
    /// [`workspace/workspaceFolders`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#workspace_workspaceFolders
    ///
    /// # Initialization
    ///
    /// If the request is sent to client before the server has been initialized, this will
    /// immediately return `Err` with JSON-RPC error code `-32002` ([read more]).
    ///
    /// [read more]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#initialize
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
    /// [`workspace/configuration`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#workspace_configuration
    ///
    /// # Initialization
    ///
    /// If the request is sent to client before the server has been initialized, this will
    /// immediately return `Err` with JSON-RPC error code `-32002` ([read more]).
    ///
    /// [read more]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#initialize
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
    /// [`workspace/applyEdit`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#workspace_applyEdit
    ///
    /// # Initialization
    ///
    /// If the request is sent to client before the server has been initialized, this will
    /// immediately return `Err` with JSON-RPC error code `-32002` ([read more]).
    ///
    /// [read more]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#initialize
    pub async fn apply_edit(&self, edit: WorkspaceEdit) -> Result<ApplyWorkspaceEditResponse> {
        self.send_request_initialized::<ApplyWorkspaceEdit>(ApplyWorkspaceEditParams { edit })
            .await
    }

    /// Submits validation diagnostics for an open file with the given URI.
    ///
    /// This corresponds to the [`textDocument/publishDiagnostics`] notification.
    ///
    /// [`textDocument/publishDiagnostics`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#textDocument_publishDiagnostics
    ///
    /// # Initialization
    ///
    /// This notification will only be sent if the server is initialized.
    pub fn publish_diagnostics(&self, uri: Url, diags: Vec<Diagnostic>, version: Option<i64>) {
        self.send_notification_initialized::<PublishDiagnostics>(PublishDiagnosticsParams::new(
            uri, diags, version,
        ));
    }

    /// Sends a custom notification to the client.
    ///
    /// # Initialization
    ///
    /// This notification will only be sent if the server is initialized.
    pub fn send_custom_notification<N>(&self, params: N::Params)
    where
        N: Notification,
    {
        self.send_notification_initialized::<N>(params);
    }

    async fn send_request<R>(&self, params: R::Params) -> Result<R::Result>
    where
        R: Request,
    {
        let id = self.inner.request_id.fetch_add(1, Ordering::Relaxed);
        let message = Outgoing::Request(make_request::<R>(id, params));

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

    fn send_notification<N>(&self, params: N::Params)
    where
        N: Notification,
    {
        let mut sender = self.inner.sender.clone();
        let message = Outgoing::Request(make_notification::<N>(params));
        tokio::spawn(async move {
            if sender.send(message).await.is_err() {
                error!("failed to send notification")
            }
        });
    }

    async fn send_request_initialized<R>(&self, params: R::Params) -> Result<R::Result>
    where
        R: Request,
    {
        if self.inner.initialized.load(Ordering::SeqCst) {
            self.send_request::<R>(params).await
        } else {
            let id = self.inner.request_id.fetch_add(1, Ordering::Relaxed);
            let msg = make_request::<R>(id, params);
            trace!("server not initialized, supressing message: {}", msg);
            Err(jsonrpc::not_initialized_error())
        }
    }

    fn send_notification_initialized<N>(&self, params: N::Params)
    where
        N: Notification,
    {
        if self.inner.initialized.load(Ordering::SeqCst) {
            self.send_notification::<N>(params);
        } else {
            let msg = make_notification::<N>(params);
            trace!("server not initialized, supressing message: {}", msg);
        }
    }
}

/// Constructs a JSON-RPC request from its corresponding LSP type.
fn make_request<R>(id: u64, params: R::Params) -> Value
where
    R: Request,
{
    serde_json::json!({
        "jsonrpc": Version,
        "id": Id::Number(id),
        "method": R::METHOD,
        "params": params,
    })
}

/// Constructs a JSON-RPC notification from its corresponding LSP type.
fn make_notification<N>(params: N::Params) -> Value
where
    N: Notification,
{
    serde_json::json!({
        "jsonrpc": Version,
        "method": N::METHOD,
        "params": params,
    })
}

#[cfg(test)]
mod tests {
    use futures::channel::mpsc;
    use futures::StreamExt;
    use serde_json::json;

    use super::*;

    async fn assert_client_messages<F: FnOnce(Client)>(f: F, expected: Outgoing) {
        let (request_tx, request_rx) = mpsc::channel(1);
        let pending = Arc::new(ClientRequests::new());

        let client = Client::new(request_tx, pending, Arc::new(AtomicBool::new(true)));
        f(client);

        let messages: Vec<_> = request_rx.collect().await;
        assert_eq!(messages, vec![expected]);
    }

    #[tokio::test]
    async fn log_message() {
        let (typ, message) = (MessageType::Log, "foo bar".to_owned());
        let expected = Outgoing::Request(make_notification::<LogMessage>(LogMessageParams {
            typ,
            message: message.clone(),
        }));

        assert_client_messages(|p| p.log_message(typ, message), expected).await;
    }

    #[tokio::test]
    async fn show_message() {
        let (typ, message) = (MessageType::Log, "foo bar".to_owned());
        let expected = Outgoing::Request(make_notification::<ShowMessage>(ShowMessageParams {
            typ,
            message: message.clone(),
        }));

        assert_client_messages(|p| p.show_message(typ, message), expected).await;
    }

    #[tokio::test]
    async fn telemetry_event() {
        let null = json!(null);
        let expected = Outgoing::Request(make_notification::<TelemetryEvent>(null.clone()));
        assert_client_messages(|p| p.telemetry_event(null), expected).await;

        let array = json!([1, 2, 3]);
        let expected = Outgoing::Request(make_notification::<TelemetryEvent>(array.clone()));
        assert_client_messages(|p| p.telemetry_event(array), expected).await;

        let object = json!({});
        let expected = Outgoing::Request(make_notification::<TelemetryEvent>(object.clone()));
        assert_client_messages(|p| p.telemetry_event(object), expected).await;

        let anything_else = json!("hello");
        let wrapped = Value::Array(vec![anything_else.clone()]);
        let expected = Outgoing::Request(make_notification::<TelemetryEvent>(wrapped));
        assert_client_messages(|p| p.telemetry_event(anything_else), expected).await;
    }

    #[tokio::test]
    async fn publish_diagnostics() {
        let uri: Url = "file:///path/to/file".parse().unwrap();
        let diagnostics = vec![Diagnostic::new_simple(Default::default(), "example".into())];

        let params = PublishDiagnosticsParams::new(uri.clone(), diagnostics.clone(), None);
        let expected = Outgoing::Request(make_notification::<PublishDiagnostics>(params));

        assert_client_messages(|p| p.publish_diagnostics(uri, diagnostics, None), expected).await;
    }
}
