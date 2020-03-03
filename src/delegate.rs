//! Type-safe wrapper for the JSON-RPC interface.

pub use self::printer::Printer;

use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::channel::mpsc::{self, Receiver};
use futures::future::{self, FutureExt, TryFutureExt};
use futures::Stream;
use jsonrpc_core::types::{ErrorCode, Params};
use jsonrpc_core::{BoxFuture, Error, Result as RpcResult};
use jsonrpc_derive::rpc;
use log::{error, info};
use lsp_types::notification::{Notification, *};
use lsp_types::request::{Request, *};
use lsp_types::*;
use serde_json::Value;

use super::LanguageServer;

mod printer;

/// Stream of notification messages produced by the language server.
#[derive(Debug)]
pub struct MessageStream(Receiver<String>);

impl Stream for MessageStream {
    type Item = String;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let recv = &mut self.as_mut().0;
        Pin::new(recv).poll_next(cx)
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

    #[rpc(name = "textDocument/willSave", raw_params)]
    fn will_save(&self, params: Params);

    #[rpc(name = "textDocument/didSave", raw_params)]
    fn did_save(&self, params: Params);

    #[rpc(name = "textDocument/didClose", raw_params)]
    fn did_close(&self, params: Params);

    // Language features

    #[rpc(name = "textDocument/completion", raw_params)]
    fn completion(&self, params: Params) -> BoxFuture<Option<CompletionResponse>>;

    #[rpc(name = "completionItem/resolve", raw_params)]
    fn completion_resolve(&self, params: Params) -> BoxFuture<CompletionItem>;

    #[rpc(name = "textDocument/hover", raw_params)]
    fn hover(&self, params: Params) -> BoxFuture<Option<Hover>>;

    #[rpc(name = "textDocument/signatureHelp", raw_params)]
    fn signature_help(&self, params: Params) -> BoxFuture<Option<SignatureHelp>>;

    #[rpc(name = "textDocument/declaration", raw_params)]
    fn goto_declaration(&self, params: Params) -> BoxFuture<Option<GotoDefinitionResponse>>;

    #[rpc(name = "textDocument/definition", raw_params)]
    fn goto_definition(&self, params: Params) -> BoxFuture<Option<GotoDefinitionResponse>>;

    #[rpc(name = "textDocument/typeDefinition", raw_params)]
    fn goto_type_definition(&self, params: Params) -> BoxFuture<Option<GotoDefinitionResponse>>;

    #[rpc(name = "textDocument/implementation", raw_params)]
    fn goto_implementation(&self, params: Params) -> BoxFuture<Option<GotoImplementationResponse>>;

    #[rpc(name = "textDocument/documentSymbol", raw_params)]
    fn document_symbol(&self, params: Params) -> BoxFuture<Option<DocumentSymbolResponse>>;

    #[rpc(name = "textDocument/documentHighlight", raw_params)]
    fn document_highlight(&self, params: Params) -> BoxFuture<Option<Vec<DocumentHighlight>>>;

    #[rpc(name = "textDocument/codeAction", raw_params)]
    fn code_action(&self, params: Params) -> BoxFuture<Option<CodeActionResponse>>;

    #[rpc(name = "textDocument/codeLens", raw_params)]
    fn code_lens(&self, params: Params) -> BoxFuture<Option<Vec<CodeLens>>>;

    #[rpc(name = "codeLens/resolve", raw_params)]
    fn code_lens_resolve(&self, params: Params) -> BoxFuture<CodeLens>;

    #[rpc(name = "textDocument/formatting", raw_params)]
    fn formatting(&self, params: Params) -> BoxFuture<Option<Vec<TextEdit>>>;
}

/// Wraps the language server backend and provides a `Printer` for sending notifications.
#[derive(Debug)]
pub struct Delegate<T> {
    // FIXME: Investigate whether `Arc` from `server` and `printer` can be removed once we switch
    // to `jsonrpsee`. These are currently necessary to resolve lifetime interaction issues between
    // `async-trait`, `jsonrpc-core`, and `.compat()`.
    //
    // https://github.com/ebkalderon/tower-lsp/issues/58
    server: Arc<T>,
    printer: Arc<Printer>,
    initialized: Arc<AtomicBool>,
}

impl<T: LanguageServer> Delegate<T> {
    /// Creates a new `Delegate` and a stream of notifications from the server to the client.
    pub fn new(server: T) -> (Self, MessageStream) {
        let (tx, rx) = mpsc::channel(1);
        let messages = MessageStream(rx);
        let initialized = Arc::new(AtomicBool::new(false));
        let delegate = Delegate {
            server: Arc::new(server),
            printer: Arc::new(Printer::new(tx, initialized.clone())),
            initialized,
        };

        (delegate, messages)
    }
}

macro_rules! delegate_notification {
    ($name:ident -> $notif:ty) => {
        fn $name(&self, params: Params) {
            if self.initialized.load(Ordering::SeqCst) {
                match params.parse() {
                    Err(err) => error!("invalid parameters for `{}`: {:?}", <$notif>::METHOD, err),
                    Ok(params) => {
                        let server = self.server.clone();
                        let printer = self.printer.clone();
                        tokio::spawn(async move { server.$name(&printer, params).await });
                    }
                }
            }
        }
    };
}

macro_rules! delegate_request {
    ($name:ident -> $request:ty) => {
        fn $name(&self, params: Params) -> BoxFuture<<$request as Request>::Result> {
            if self.initialized.load(Ordering::SeqCst) {
                let server = self.server.clone();
                let fut = async move {
                    match params.parse() {
                        Ok(params) => server.$name(params).await,
                        Err(err) => Err(Error::invalid_params_with_details(
                            "invalid parameters",
                            err,
                        )),
                    }
                };

                Box::new(fut.boxed().compat())
            } else {
                Box::new(future::err(not_initialized_error()).compat())
            }
        }
    };
}

impl<T: LanguageServer> LanguageServerCore for Delegate<T> {
    fn initialize(&self, params: Params) -> RpcResult<InitializeResult> {
        let params: InitializeParams = params.parse()?;
        let response = self.server.initialize(&self.printer, params)?;
        info!("language server initialized");
        self.initialized.store(true, Ordering::SeqCst);
        Ok(response)
    }

    delegate_notification!(initialized -> Initialized);

    fn shutdown(&self) -> BoxFuture<()> {
        if self.initialized.load(Ordering::SeqCst) {
            let server = self.server.clone();
            Box::new(async move { server.shutdown().await }.boxed().compat())
        } else {
            Box::new(future::err(not_initialized_error()).compat())
        }
    }

    delegate_notification!(did_change_workspace_folders -> DidChangeWorkspaceFolders);
    delegate_notification!(did_change_configuration -> DidChangeConfiguration);
    delegate_notification!(did_change_watched_files -> DidChangeWatchedFiles);
    delegate_request!(symbol -> WorkspaceSymbol);

    fn execute_command(&self, params: Params) -> BoxFuture<Option<Value>> {
        if self.initialized.load(Ordering::SeqCst) {
            let server = self.server.clone();
            let printer = self.printer.clone();
            let fut = async move {
                match params.parse() {
                    Ok(params) => server.execute_command(&printer, params).await,
                    Err(err) => Err(Error::invalid_params_with_details(
                        "invalid parameters",
                        err,
                    )),
                }
            };

            Box::new(fut.boxed().compat())
        } else {
            Box::new(future::err(not_initialized_error()).compat())
        }
    }

    delegate_notification!(did_open -> DidOpenTextDocument);
    delegate_notification!(did_change -> DidChangeTextDocument);
    delegate_notification!(will_save -> WillSaveTextDocument);
    delegate_notification!(did_save -> DidSaveTextDocument);
    delegate_notification!(did_close -> DidCloseTextDocument);

    delegate_request!(completion -> Completion);
    delegate_request!(completion_resolve -> ResolveCompletionItem);
    delegate_request!(hover -> HoverRequest);
    delegate_request!(signature_help -> SignatureHelpRequest);
    delegate_request!(goto_declaration -> GotoDeclaration);
    delegate_request!(goto_definition -> GotoDefinition);
    delegate_request!(goto_type_definition -> GotoTypeDefinition);
    delegate_request!(goto_implementation -> GotoImplementation);
    delegate_request!(document_symbol -> DocumentSymbolRequest);
    delegate_request!(document_highlight -> DocumentHighlightRequest);
    delegate_request!(code_action -> CodeActionRequest);
    delegate_request!(code_lens -> CodeLensRequest);
    delegate_request!(code_lens_resolve -> CodeLensResolve);
    delegate_request!(formatting -> Formatting);
}

/// Error response returned for every request received before the server is initialized.
///
/// See [here](https://microsoft.github.io/language-server-protocol/specifications/specification-current/#initialize)
/// for reference.
fn not_initialized_error() -> Error {
    Error {
        code: ErrorCode::ServerError(-32002),
        message: "Server not initialized".to_string(),
        data: None,
    }
}
