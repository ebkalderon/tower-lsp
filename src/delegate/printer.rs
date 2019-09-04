//! Types for sending data back to the language client.

use std::fmt::Display;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use futures::sync::mpsc::Sender;
use futures::{Future, Sink};
use jsonrpc_core::types::{request, Id, Version};
use log::{error, trace};
use lsp_types::notification::{Notification, *};
use lsp_types::request::{ApplyWorkspaceEdit, RegisterCapability, Request, UnregisterCapability};
use lsp_types::*;
use serde::Serialize;
use serde_json::Value;

/// Sends notifications from the language server to the client.
#[derive(Debug)]
pub struct Printer {
    buffer: Sender<String>,
    initialized: Arc<AtomicBool>,
    request_id: AtomicU64,
}

impl Printer {
    pub(super) fn new(buffer: Sender<String>, initialized: Arc<AtomicBool>) -> Self {
        Printer {
            buffer,
            initialized,
            request_id: AtomicU64::new(0),
        }
    }

    /// Notifies the client to log a particular message.
    ///
    /// This corresponds to the [`window/logMessage`] notification.
    ///
    /// [`window/logMessage`]: https://microsoft.github.io/language-server-protocol/specification#window_logMessage
    pub fn log_message<M: Display>(&self, typ: MessageType, message: M) {
        self.send_message(make_notification::<LogMessage>(LogMessageParams {
            typ,
            message: message.to_string(),
        }));
    }

    /// Notifies the client to display a particular message in the user interface.
    ///
    /// This corresponds to the [`window/showMessage`] notification.
    ///
    /// [`window/showMessage`]: https://microsoft.github.io/language-server-protocol/specification#window_showMessage
    pub fn show_message<M: Display>(&self, typ: MessageType, message: M) {
        self.send_message(make_notification::<ShowMessage>(ShowMessageParams {
            typ,
            message: message.to_string(),
        }));
    }

    /// Notifies the client to log a telemetry event.
    ///
    /// This corresponds to the [`telemetry/event`] notification.
    ///
    /// [`telemetry/event`]: https://microsoft.github.io/language-server-protocol/specification#telemetry_event
    pub fn telemetry_event<S: Serialize>(&self, data: S) {
        match serde_json::to_value(data) {
            Err(e) => error!("invalid JSON in `telemetry/event` notification: {}", e),
            Ok(value) => {
                if !value.is_array() && !value.is_object() {
                    let value = Value::Array(vec![value]);
                    self.send_message(make_notification::<TelemetryEvent>(value));
                } else {
                    self.send_message(make_notification::<TelemetryEvent>(value));
                }
            }
        }
    }

    /// Register a new capability with the client.
    ///
    /// This corresponds to the [`client/registerCapability`] request.
    ///
    /// [`client/registerCapability`]: https://microsoft.github.io/language-server-protocol/specification#client_registerCapability
    pub fn register_capability(&self, registrations: Vec<Registration>) {
        // FIXME: Check whether the request succeeded or failed.
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);
        self.send_message(make_request::<RegisterCapability>(
            id,
            RegistrationParams { registrations },
        ))
    }

    /// Unregister a capability with the client.
    ///
    /// This corresponds to the [`client/unregisterCapability`] request.
    ///
    /// [`client/unregisterCapability`]: https://microsoft.github.io/language-server-protocol/specification#client_unregisterCapability
    pub fn unregister_capability(&self, unregisterations: Vec<Unregistration>) {
        // FIXME: Check whether the request succeeded or failed.
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);
        self.send_message(make_request::<UnregisterCapability>(
            id,
            UnregistrationParams { unregisterations },
        ))
    }

    /// Requests a workspace resource be edited on the client side.
    ///
    /// This corresponds to the [`workspace/applyEdit`] request.
    ///
    /// [`workspace/applyEdit`]: https://microsoft.github.io/language-server-protocol/specification#workspace_applyEdit
    pub fn apply_edit(&self, edit: WorkspaceEdit) -> bool {
        // FIXME: Check whether the request succeeded or failed and retrieve apply status.
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);
        self.send_message(make_request::<ApplyWorkspaceEdit>(
            id,
            ApplyWorkspaceEditParams { edit },
        ));
        true
    }

    /// Submits validation diagnostics for an open file with the given URI.
    ///
    /// This corresponds to the [`textDocument/publishDiagnostics`] notification.
    ///
    /// [`textDocument/publishDiagnostics`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_publishDiagnostics
    pub fn publish_diagnostics(&self, uri: Url, diagnostics: Vec<Diagnostic>) {
        self.send_message(make_notification::<PublishDiagnostics>(
            PublishDiagnosticsParams::new(uri, diagnostics),
        ));
    }

    fn send_message(&self, message: String) {
        if self.initialized.load(Ordering::SeqCst) {
            tokio_executor::spawn(
                self.buffer
                    .clone()
                    .send(message)
                    .map(|_| ())
                    .map_err(|_| error!("failed to send message")),
            );
        } else {
            trace!("server not initialized, supressing message: {}", message);
        }
    }
}

/// Constructs a JSON-RPC request from its corresponding LSP type.
fn make_request<N>(id: u64, params: N::Params) -> String
where
    N: Request,
    N::Params: Serialize,
{
    // Since these types come from the `lsp-types` crate and validity is enforced via the
    // `Notification` trait, the `unwrap()` calls below should never fail.
    let output = serde_json::to_string(&params).unwrap();
    let params = serde_json::from_str(&output).unwrap();
    serde_json::to_string(&request::MethodCall {
        jsonrpc: Some(Version::V2),
        id: Id::Num(id),
        method: N::METHOD.to_owned(),
        params,
    })
    .unwrap()
}

/// Constructs a JSON-RPC notification from its corresponding LSP type.
fn make_notification<N>(params: N::Params) -> String
where
    N: Notification,
    N::Params: Serialize,
{
    // Since these types come from the `lsp-types` crate and validity is enforced via the
    // `Notification` trait, the `unwrap()` calls below should never fail.
    let output = serde_json::to_string(&params).unwrap();
    let params = serde_json::from_str(&output).unwrap();
    serde_json::to_string(&request::Notification {
        jsonrpc: Some(Version::V2),
        method: N::METHOD.to_owned(),
        params,
    })
    .unwrap()
}

#[cfg(test)]
mod tests {
    use futures::{future, sync::mpsc, Stream};
    use tokio::runtime::current_thread;

    use super::*;

    fn assert_printer_messages<F: FnOnce(Printer)>(f: F, expected: String) {
        let (tx, rx) = mpsc::channel(1);
        let printer = Printer::new(tx, Arc::new(AtomicBool::new(true)));

        current_thread::block_on_all(
            future::lazy(move || {
                f(printer);
                rx.collect()
            })
            .and_then(move |messages| {
                assert_eq!(messages, vec![expected]);
                Ok(())
            }),
        )
        .unwrap();
    }

    #[test]
    fn publish_diagnostics() {
        let uri: Url = "file:///path/to/file".parse().unwrap();
        let diagnostics = vec![Diagnostic::new_simple(Default::default(), "example".into())];

        let params = PublishDiagnosticsParams::new(uri.clone(), diagnostics.clone());
        let expected = make_notification::<PublishDiagnostics>(params);

        assert_printer_messages(|p| p.publish_diagnostics(uri, diagnostics), expected);
    }

    #[test]
    fn log_message() {
        let (typ, message) = (MessageType::Log, "foo bar".to_owned());
        let expected = make_notification::<LogMessage>(LogMessageParams {
            typ,
            message: message.clone(),
        });

        assert_printer_messages(|p| p.log_message(typ, message), expected);
    }

    #[test]
    fn show_message() {
        let (typ, message) = (MessageType::Log, "foo bar".to_owned());
        let expected = make_notification::<ShowMessage>(ShowMessageParams {
            typ,
            message: message.clone(),
        });

        assert_printer_messages(|p| p.show_message(typ, message), expected);
    }
}
