//! Type-safe wrapper for the JSON-RPC interface.

pub use self::printer::Printer;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use futures::sync::mpsc::{self, Receiver};
use futures::{future, Poll, Stream};
use jsonrpc_core::types::{ErrorCode, Params};
use jsonrpc_core::{BoxFuture, Error, Result as RpcResult};
use jsonrpc_derive::rpc;
use log::{error, trace};
use lsp_types::notification::{Notification, *};
use lsp_types::request::{Request, *};
use lsp_types::*;
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::LanguageServer;

mod printer;

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

    // Workspace

    #[rpc(name = "workspace/didChangeWorkspaceFolders", raw_params)]
    fn did_change_workspace_folders(&self, params: Params);

    #[rpc(name = "workspace/DidChangeConfiguration", raw_params)]
    fn did_change_configuration(&self, params: Params);

    #[rpc(name = "workspace/didChangeWatchedFiles", raw_params)]
    fn did_change_watched_files(&self, params: Params);

    #[rpc(name = "workspace/symbol", raw_params)]
    fn symbol(&self, params: Params) -> BoxFuture<Option<Vec<SymbolInformation>>>;

    #[rpc(name = "workspace/executeCommand", raw_params)]
    fn execute_command(&self, params: Params) -> BoxFuture<Option<Value>>;

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

    #[rpc(name = "textDocument/completion", raw_params)]
    fn completion(&self, params: Params) -> BoxFuture<Option<CompletionResponse>>;

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

    fn delegate_notification<N, F>(&self, params: Params, delegate: F)
    where
        N: Notification,
        N::Params: DeserializeOwned,
        F: Fn(&Printer, N::Params),
    {
        trace!("received `{}` notification: {:?}", N::METHOD, params);
        if self.initialized.load(Ordering::SeqCst) {
            match params.parse::<N::Params>() {
                Ok(params) => delegate(&self.printer, params),
                Err(err) => error!("invalid parameters for `{}`: {:?}", N::METHOD, err),
            }
        }
    }

    fn delegate_request<R, F>(&self, params: Params, delegate: F) -> BoxFuture<R::Result>
    where
        R: Request,
        R::Params: DeserializeOwned,
        R::Result: Send + 'static,
        F: Fn(R::Params) -> BoxFuture<R::Result>,
    {
        trace!("received `{}` request: {:?}", R::METHOD, params);
        if self.initialized.load(Ordering::SeqCst) {
            match params.parse() {
                Ok(params) => delegate(params),
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

impl<T: LanguageServer> LanguageServerCore for Delegate<T> {
    fn initialize(&self, params: Params) -> RpcResult<InitializeResult> {
        trace!("received `initialize` request: {:?}", params);
        let params: InitializeParams = params.parse()?;
        let response = self.server.initialize(&self.printer, params)?;
        self.initialized.store(true, Ordering::SeqCst);
        Ok(response)
    }

    fn initialized(&self, params: Params) {
        self.delegate_notification::<Initialized, _>(params, |p, params| {
            self.server.initialized(p, params)
        });
    }

    fn shutdown(&self) -> BoxFuture<()> {
        trace!("received `shutdown` request");
        if self.initialized.load(Ordering::SeqCst) {
            Box::new(self.server.shutdown())
        } else {
            Box::new(future::err(not_initialized_error()))
        }
    }

    fn did_change_workspace_folders(&self, params: Params) {
        self.delegate_notification::<DidChangeWorkspaceFolders, _>(params, |p, params| {
            self.server.did_change_workspace_folders(p, params)
        });
    }

    fn did_change_configuration(&self, params: Params) {
        self.delegate_notification::<DidChangeConfiguration, _>(params, |p, params| {
            self.server.did_change_configuration(p, params)
        });
    }

    fn did_change_watched_files(&self, params: Params) {
        self.delegate_notification::<DidChangeWatchedFiles, _>(params, |p, params| {
            self.server.did_change_watched_files(p, params)
        });
    }

    fn symbol(&self, params: Params) -> BoxFuture<Option<Vec<SymbolInformation>>> {
        self.delegate_request::<WorkspaceSymbol, _>(params, |p| Box::new(self.server.symbol(p)))
    }

    fn execute_command(&self, params: Params) -> BoxFuture<Option<Value>> {
        self.delegate_request::<ExecuteCommand, _>(params, |p| {
            Box::new(self.server.execute_command(&self.printer, p))
        })
    }

    fn did_open(&self, params: Params) {
        self.delegate_notification::<DidOpenTextDocument, _>(params, |p, params| {
            self.server.did_open(p, params)
        });
    }

    fn did_change(&self, params: Params) {
        self.delegate_notification::<DidChangeTextDocument, _>(params, |p, params| {
            self.server.did_change(p, params)
        });
    }

    fn did_save(&self, params: Params) {
        self.delegate_notification::<DidSaveTextDocument, _>(params, |p, params| {
            self.server.did_save(p, params)
        });
    }

    fn did_close(&self, params: Params) {
        self.delegate_notification::<DidCloseTextDocument, _>(params, |p, params| {
            self.server.did_close(p, params)
        });
    }

    fn completion(&self, params: Params) -> BoxFuture<Option<CompletionResponse>> {
        self.delegate_request::<Completion, _>(params, |p| Box::new(self.server.completion(p)))
    }

    fn hover(&self, params: Params) -> BoxFuture<Option<Hover>> {
        self.delegate_request::<HoverRequest, _>(params, |p| Box::new(self.server.hover(p)))
    }

    fn document_highlight(&self, params: Params) -> BoxFuture<Option<Vec<DocumentHighlight>>> {
        self.delegate_request::<DocumentHighlightRequest, _>(params, |p| {
            Box::new(self.server.document_highlight(p))
        })
    }
}

/// Error response returned for every request received before the server is initialized.
///
/// See [here](https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#initialize)
/// for reference.
fn not_initialized_error() -> Error {
    Error {
        code: ErrorCode::ServerError(-32002),
        message: "Server not initialized".to_string(),
        data: None,
    }
}
