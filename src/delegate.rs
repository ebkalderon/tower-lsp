//! Type-safe wrapper for the JSON-RPC interface.

pub use self::printer::{MessageStream, Printer};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use futures::{future, sync::mpsc};
use jsonrpc_core::types::{ErrorCode, Params};
use jsonrpc_core::{BoxFuture, Error, Result as RpcResult};
use jsonrpc_derive::rpc;
use log::{error, trace};
use lsp_types::*;

use super::LanguageServer;

mod printer;

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
