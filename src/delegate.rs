//! Type-safe wrapper for the JSON-RPC interface.

use std::fmt::Display;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use futures::sync::mpsc::{self, Receiver, Sender};
use futures::{future, Future, Poll, Sink, Stream};
use jsonrpc_core::types::{request, ErrorCode, Params, Version};
use jsonrpc_core::{BoxFuture, Error, Result as RpcResult};
use jsonrpc_derive::rpc;
use log::{error, trace};
use lsp_types::notification::{LogMessage, Notification, PublishDiagnostics, ShowMessage};
use lsp_types::*;
use serde::Serialize;

use super::LanguageServer;

/// Stream of notification messages produced by the language server.
#[derive(Debug)]
pub struct MessageStream(Receiver<String>);

impl Stream for MessageStream {
    type Item = String;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<String>, ()> {
        self.0.poll()
    }
}

/// Sends notifications from the language server to the client.
#[derive(Debug)]
pub struct Printer {
    buffer: Sender<String>,
    initialized: Arc<AtomicBool>,
}

impl Printer {
    fn new(buffer: Sender<String>, initialized: Arc<AtomicBool>) -> Self {
        Printer {
            buffer,
            initialized,
        }
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

/// JSON-RPC interface used by the Language Server Protocol.
#[rpc(server)]
pub trait LanguageServerCore {
    // Initialization

    #[rpc(name = "initialize", raw_params)]
    fn initialize(&self, params: Params) -> RpcResult<InitializeResult>;

    #[rpc(name = "initialized", raw_params)]
    fn initialized(&self, params: Params);

    #[rpc(name = "shutdown")]
    fn shutdown(&self) -> BoxFuture<()>;

    // Text synchronization

    #[rpc(name = "textDocument/didOpen", raw_params)]
    fn did_open(&self, params: Params);

    #[rpc(name = "textDocument/didChange", raw_params)]
    fn did_change(&self, params: Params);

    #[rpc(name = "textDocument/didSave", raw_params)]
    fn did_save(&self, params: Params);

    #[rpc(name = "textDocument/didClose", raw_params)]
    fn did_close(&self, params: Params);

    // Language features

    #[rpc(name = "textDocument/hover", raw_params)]
    fn hover(&self, params: Params) -> BoxFuture<Option<Hover>>;

    #[rpc(name = "textDocument/documentHighlight", raw_params)]
    fn document_highlight(&self, params: Params) -> BoxFuture<Option<Vec<DocumentHighlight>>>;
}

/// Wraps the language server backend and provides a `Printer` for sending notifications.
#[derive(Debug)]
pub struct Delegate<T> {
    server: T,
    printer: Printer,
    initialized: Arc<AtomicBool>,
}

impl<T: LanguageServer> Delegate<T> {
    /// Creates a new `Delegate` and a stream of notifications from the server to the client.
    pub fn new(server: T) -> (Self, MessageStream) {
        let (tx, rx) = mpsc::channel(1);
        let messages = MessageStream(rx);
        let initialized = Arc::new(AtomicBool::new(false));
        let delegate = Delegate {
            server,
            printer: Printer::new(tx, initialized.clone()),
            initialized,
        };

        (delegate, messages)
    }
}

impl<T: LanguageServer> LanguageServerCore for Delegate<T> {
    fn initialize(&self, params: Params) -> RpcResult<InitializeResult> {
        trace!("received `initialize` request: {:?}", params);
        let params: InitializeParams = params.parse()?;
        let response = self.server.initialize(params)?;
        self.initialized.store(true, Ordering::SeqCst);
        Ok(response)
    }

    fn initialized(&self, params: Params) {
        trace!("received `initialized` notification: {:?}", params);
        if self.initialized.load(Ordering::SeqCst) {
            match params.parse::<InitializedParams>() {
                Ok(params) => self.server.initialized(&self.printer, params),
                Err(err) => error!("invalid parameters for `initialized`: {:?}", err),
            }
        }
    }

    fn shutdown(&self) -> BoxFuture<()> {
        trace!("received `shutdown` request");
        if self.initialized.load(Ordering::SeqCst) {
            Box::new(self.server.shutdown())
        } else {
            Box::new(future::err(not_initialized_error()))
        }
    }

    fn did_open(&self, params: Params) {
        trace!("received `textDocument/didOpen` notification: {:?}", params);
        if self.initialized.load(Ordering::SeqCst) {
            match params.parse::<DidOpenTextDocumentParams>() {
                Ok(params) => self.server.did_open(&self.printer, params),
                Err(err) => error!("invalid parameters for `textDocument/didOpen`: {:?}", err),
            }
        }
    }

    fn did_change(&self, params: Params) {
        trace!(
            "received `textDocument/didChange` notification: {:?}",
            params
        );
        if self.initialized.load(Ordering::SeqCst) {
            match params.parse::<DidChangeTextDocumentParams>() {
                Ok(params) => self.server.did_change(&self.printer, params),
                Err(err) => error!("invalid parameters for `textDocument/didChange`: {:?}", err),
            }
        }
    }

    fn did_save(&self, params: Params) {
        trace!("received `textDocument/didSave` notification: {:?}", params);
        if self.initialized.load(Ordering::SeqCst) {
            match params.parse::<DidSaveTextDocumentParams>() {
                Ok(params) => self.server.did_save(&self.printer, params),
                Err(err) => error!("invalid parameters for `textDocument/didSave`: {:?}", err),
            }
        }
    }

    fn did_close(&self, params: Params) {
        trace!(
            "received `textDocument/didClose` notification: {:?}",
            params
        );
        if self.initialized.load(Ordering::SeqCst) {
            match params.parse::<DidCloseTextDocumentParams>() {
                Ok(params) => self.server.did_close(&self.printer, params),
                Err(err) => error!("invalid parameters for `textDocument/didClose`: {:?}", err),
            }
        }
    }

    fn hover(&self, params: Params) -> BoxFuture<Option<Hover>> {
        trace!("received `textDocument/hover` request: {:?}", params);
        if self.initialized.load(Ordering::SeqCst) {
            match params.parse::<TextDocumentPositionParams>() {
                Ok(params) => Box::new(self.server.hover(params)),
                Err(err) => Box::new(future::err(Error::invalid_params_with_details(
                    "invalid parameters",
                    err,
                ))),
            }
        } else {
            Box::new(future::err(not_initialized_error()))
        }
    }

    fn document_highlight(&self, params: Params) -> BoxFuture<Option<Vec<DocumentHighlight>>> {
        trace!(
            "received `textDocument/documentHighlight` request: {:?}",
            params
        );
        if self.initialized.load(Ordering::SeqCst) {
            match params.parse::<TextDocumentPositionParams>() {
                Ok(params) => Box::new(self.server.document_highlight(params)),
                Err(err) => Box::new(future::err(Error::invalid_params_with_details(
                    "invalid parameters",
                    err,
                ))),
            }
        } else {
            Box::new(future::err(not_initialized_error()))
        }
    }
}

/// Error response returned for every request received before the server is initialized.
///
/// See [here](https://microsoft.github.io/language-server-protocol/specification#initialize) for
/// reference.
fn not_initialized_error() -> Error {
    Error::new(ErrorCode::ServerError(-32002))
}

#[cfg(test)]
mod tests {
    use tokio::runtime::current_thread;

    use super::*;

    fn assert_printer_messages<F: FnOnce(Printer)>(f: F, expected: String) {
        let (tx, rx) = mpsc::channel(1);
        let messages = MessageStream(rx);
        let printer = Printer::new(tx, Arc::new(AtomicBool::new(true)));

        current_thread::block_on_all(
            future::lazy(move || {
                f(printer);
                messages.collect()
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
