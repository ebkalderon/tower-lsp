//! Type-safe wrapper for the JSON-RPC interface.

use std::fmt::Display;

use futures::sync::mpsc::{self, Receiver, Sender};
use futures::{future, Future, Poll, Sink, Stream};
use jsonrpc_core::types::params::Params;
use jsonrpc_core::{BoxFuture, Error, Result};
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
pub struct Printer(Sender<String>);

impl Printer {
    /// Submits validation diagnostics for an open file with the given URI.
    ///
    /// This corresponds to the [`textDocument/publishDiagnostics`] notification.
    ///
    /// [`textDocument/publishDiagnostics`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_publishDiagnostics
    pub fn publish_diagnostics(&self, uri: Url, diagnostics: Vec<Diagnostic>) {
        let params = PublishDiagnosticsParams::new(uri, diagnostics);
        self.send_notification(PublishDiagnostics::METHOD, params);
    }

    /// Notifies the client to log a particular message.
    ///
    /// This corresponds to the [`window/logMessage`] notification.
    ///
    /// [`window/logMessage`]: https://microsoft.github.io/language-server-protocol/specification#window_logMessage
    pub fn log_message<M: Display>(&self, typ: MessageType, message: M) {
        self.send_notification(
            LogMessage::METHOD,
            LogMessageParams {
                typ,
                message: message.to_string(),
            },
        );
    }

    /// Notifies the client to display a particular message in the user interface.
    ///
    /// This corresponds to the [`window/showMessage`] notification.
    ///
    /// [`window/showMessage`]: https://microsoft.github.io/language-server-protocol/specification#window_showMessage
    pub fn show_message<M: Display>(&self, typ: MessageType, message: M) {
        self.send_notification(
            ShowMessage::METHOD,
            ShowMessageParams {
                typ,
                message: message.to_string(),
            },
        );
    }

    fn send_notification<S: Serialize>(&self, method: &str, params: S) {
        match serde_json::to_string(&params) {
            Err(err) => error!("failed to serialize message for `{}`: {}", method, err),
            Ok(params) => {
                let message = format!(
                    r#"{{"jsonrpc":"2.0","method":"{}","params":{}}}"#,
                    method, params
                );
                tokio_executor::spawn(
                    self.0
                        .clone()
                        .send(message)
                        .map(|_| ())
                        .map_err(|_| error!("failed to send message")),
                );
            }
        }
    }
}

/// JSON-RPC interface used by the Language Server Protocol.
#[rpc(server)]
pub trait LanguageServerCore {
    type ShutdownFuture: Future<Item = (), Error = Error> + Send;

    // Initialization

    #[rpc(name = "initialize", raw_params)]
    fn initialize(&self, params: Params) -> Result<InitializeResult>;

    #[rpc(name = "initialized", raw_params)]
    fn initialized(&self, params: Params);

    #[rpc(name = "shutdown", returns = "()")]
    fn shutdown(&self) -> Self::ShutdownFuture;

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

    #[rpc(name = "textDocument/hover", raw_params, returns = "Option<Hover>")]
    fn hover(&self, params: Params) -> BoxFuture<Option<Hover>>;

    #[rpc(
        name = "textDocument/documentHighlight",
        raw_params,
        returns = "Option<Vec<DocumentHighlight>>"
    )]
    fn highlight(&self, params: Params) -> BoxFuture<Option<Vec<DocumentHighlight>>>;
}

/// Wraps the language server backend and provides a `Printer` for sending notifications.
#[derive(Debug)]
pub struct Delegate<T> {
    server: T,
    printer: Printer,
}

impl<T: LanguageServer> Delegate<T> {
    /// Creates a new `Delegate` and a stream of notifications from the server to the client.
    pub fn new(server: T) -> (Self, MessageStream) {
        let (tx, rx) = mpsc::channel(1);
        let messages = MessageStream(rx);
        let printer = Printer(tx);
        (Delegate { server, printer }, messages)
    }
}

impl<T: LanguageServer> LanguageServerCore for Delegate<T> {
    type ShutdownFuture = T::ShutdownFuture;

    fn initialize(&self, params: Params) -> Result<InitializeResult> {
        trace!("received `initialize` request: {:?}", params);
        let params: InitializeParams = params.parse()?;
        self.server.initialize(params)
    }

    fn initialized(&self, params: Params) {
        trace!("received `initialized` notification: {:?}", params);
        match params.parse::<InitializedParams>() {
            Ok(params) => self.server.initialized(&self.printer, params),
            Err(err) => error!("invalid parameters for `initialized`: {:?}", err),
        }
    }

    fn shutdown(&self) -> Self::ShutdownFuture {
        trace!("received `shutdown` request");
        self.server.shutdown()
    }

    fn did_open(&self, params: Params) {
        trace!("received `textDocument/didOpen` notification: {:?}", params);
        match params.parse::<DidOpenTextDocumentParams>() {
            Ok(params) => self.server.did_open(&self.printer, params),
            Err(err) => error!("invalid parameters for `textDocument/didOpen`: {:?}", err),
        }
    }

    fn did_change(&self, params: Params) {
        trace!(
            "received `textDocument/didChange` notification: {:?}",
            params
        );
        match params.parse::<DidChangeTextDocumentParams>() {
            Ok(params) => self.server.did_change(&self.printer, params),
            Err(err) => error!("invalid parameters for `textDocument/didChange`: {:?}", err),
        }
    }

    fn did_save(&self, params: Params) {
        trace!("received `textDocument/didSave` notification: {:?}", params);
        match params.parse::<DidSaveTextDocumentParams>() {
            Ok(params) => self.server.did_save(&self.printer, params),
            Err(err) => error!("invalid parameters for `textDocument/didSave`: {:?}", err),
        }
    }

    fn did_close(&self, params: Params) {
        trace!(
            "received `textDocument/didClose` notification: {:?}",
            params
        );
        match params.parse::<DidCloseTextDocumentParams>() {
            Ok(params) => self.server.did_close(&self.printer, params),
            Err(err) => error!("invalid parameters for `textDocument/didClose`: {:?}", err),
        }
    }

    fn hover(&self, params: Params) -> BoxFuture<Option<Hover>> {
        trace!("received `textDocument/hover` request: {:?}", params);
        match params.parse::<TextDocumentPositionParams>() {
            Ok(params) => Box::new(self.server.hover(params)),
            Err(err) => Box::new(future::err(Error::invalid_params_with_details(
                "invalid parameters",
                err,
            ))),
        }
    }

    fn highlight(&self, params: Params) -> BoxFuture<Option<Vec<DocumentHighlight>>> {
        trace!("received `textDocument/highlight` request: {:?}", params);
        match params.parse::<TextDocumentPositionParams>() {
            Ok(params) => Box::new(self.server.highlight(params)),
            Err(err) => Box::new(future::err(Error::invalid_params_with_details(
                "invalid parameters",
                err,
            ))),
        }
    }
}
