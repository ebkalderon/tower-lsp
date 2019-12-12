//! Language Server Protocol (LSP) server abstraction for [Tower].
//!
//! [Tower]: https://github.com/tower-rs/tower
//!
//! # Example
//!
//! ```rust
//! # use futures::future;
//! # use jsonrpc_core::{BoxFuture, Result};
//! # use serde_json::Value;
//! # use tower_lsp::lsp_types::*;
//! # use tower_lsp::{LanguageServer, LspService, Printer, Server};
//! #
//! #[derive(Debug, Default)]
//! struct Backend;
//!
//! impl LanguageServer for Backend {
//!     type ShutdownFuture = BoxFuture<()>;
//!     type SymbolFuture = BoxFuture<Option<Vec<SymbolInformation>>>;
//!     type ExecuteFuture = BoxFuture<Option<Value>>;
//!     type CompletionFuture = BoxFuture<Option<CompletionResponse>>;
//!     type HoverFuture = BoxFuture<Option<Hover>>;
//!     type HighlightFuture = BoxFuture<Option<Vec<DocumentHighlight>>>;
//!
//!     fn initialize(&self, _: &Printer, _: InitializeParams) -> Result<InitializeResult> {
//!         Ok(InitializeResult::default())
//!     }
//!
//!     fn initialized(&self, printer: &Printer, _: InitializedParams) {
//!         printer.log_message(MessageType::Info, "server initialized!");
//!     }
//!
//!     fn shutdown(&self) -> Self::ShutdownFuture {
//!         Box::new(future::ok(()))
//!     }
//!
//!     fn symbol(&self, _: WorkspaceSymbolParams) -> Self::SymbolFuture {
//!         Box::new(future::ok(None))
//!     }
//!
//!     fn execute_command(&self, _: &Printer, _: ExecuteCommandParams) -> Self::ExecuteFuture {
//!         Box::new(future::ok(None))
//!     }
//!
//!     fn completion(&self, _: CompletionParams) -> Self::CompletionFuture {
//!         Box::new(future::ok(None))
//!     }
//!
//!     fn hover(&self, _: TextDocumentPositionParams) -> Self::HoverFuture {
//!         Box::new(future::ok(None))
//!     }
//!
//!     fn document_highlight(&self, _: TextDocumentPositionParams) -> Self::HighlightFuture {
//!         Box::new(future::ok(None))
//!     }
//! }
//!
//! fn main() {
//!     let stdin = tokio::io::stdin();
//!     let stdout = tokio::io::stdout();
//!
//!     let (service, messages) = LspService::new(Backend::default());
//!     let handle = service.close_handle();
//!     let server = Server::new(stdin, stdout)
//!         .interleave(messages)
//!         .serve(service);
//!
//!     tokio::run(handle.run_until_exit(server));
//! }
//! ```

#![deny(missing_debug_implementations)]
#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub extern crate lsp_types;

pub use self::delegate::{MessageStream, Printer};
pub use self::message::Incoming;
pub use self::service::{ExitReceiver, ExitedError, LspService};
pub use self::stdio::Server;

use futures::Future;
use jsonrpc_core::{Error, Result};
use lsp_types::*;
use serde_json::Value;

mod codec;
mod delegate;
mod message;
mod service;
mod stdio;

/// Trait implemented by language server backends.
///
/// This interface allows servers adhering to the [Language Server Protocol] to be implemented in a
/// safe and easily testable way without exposing the low-level implementation details.
///
/// [Language Server Protocol]: https://microsoft.github.io/language-server-protocol/
pub trait LanguageServer: Send + Sync + 'static {
    /// Response returned when a server shutdown is requested.
    type ShutdownFuture: Future<Item = (), Error = Error> + Send;
    /// Response returned when a workspace symbol action is requested.
    type SymbolFuture: Future<Item = Option<Vec<SymbolInformation>>, Error = Error> + Send;
    /// Response returned when an execute command action is requested.
    type ExecuteFuture: Future<Item = Option<Value>, Error = Error> + Send;
    /// Response returned when a completion action is requested.
    type CompletionFuture: Future<Item = Option<CompletionResponse>, Error = Error> + Send;
    /// Response returned when a hover action is requested.
    type HoverFuture: Future<Item = Option<Hover>, Error = Error> + Send;
    /// Response returned when a document highlight action is requested.
    type HighlightFuture: Future<Item = Option<Vec<DocumentHighlight>>, Error = Error> + Send;

    /// The [`initialize`] request is the first request sent from the client to the server.
    ///
    /// [`initialize`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#initialize
    fn initialize(&self, printer: &Printer, params: InitializeParams) -> Result<InitializeResult>;

    /// The [`initialized`] notification is sent from the client to the server after the client
    /// received the result of the initialize request but before the client sends anything else.
    ///
    /// The server can use the `initialized` notification for example to dynamically register
    /// capabilities with the client.
    ///
    /// [`initialized`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#initialized
    fn initialized(&self, printer: &Printer, params: InitializedParams) {
        let _ = printer;
        let _ = params;
    }

    /// The [`shutdown`] request asks the server to gracefully shut down, but to not exit.
    ///
    /// This request is often later followed by an [`exit`] notification, which will cause the
    /// server to exit immediately.
    ///
    /// [`shutdown`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#shutdown
    /// [`exit`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#exit
    fn shutdown(&self) -> Self::ShutdownFuture;

    /// The [`workspace/didChangeWorkspaceFolders`] notification is sent from the client to the
    /// server to inform about workspace folder configuration changes.
    ///
    /// The notification is sent by default if both of these boolean fields were set to `true` in
    /// the [`initialize`] method:
    ///
    /// * `InitializeParams::capabilities::workspace::workspace_folders`
    /// * `InitializeResult::capabilities::workspace::workspace_folders::supported`
    ///
    /// This notification is also sent if the server has registered itself to receive this
    /// notification.
    ///
    /// [`workspace/didChangeWorkspaceFolders`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#workspace_didChangeWorkspaceFolders
    /// [`initialize`]: #tymethod.initialize
    fn did_change_workspace_folders(&self, p: &Printer, params: DidChangeWorkspaceFoldersParams) {
        let _ = p;
        let _ = params;
    }

    /// The [`workspace/didChangeConfiguration`] notification is sent from the client to the server
    /// to signal the change of configuration settings.
    ///
    /// [`workspace/didChangeConfiguration`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#workspace_didChangeConfiguration
    fn did_change_configuration(&self, printer: &Printer, params: DidChangeConfigurationParams) {
        let _ = printer;
        let _ = params;
    }

    /// The [`workspace/didChangeWatchedFiles`] notification is sent from the client to the server
    /// when the client detects changes to files watched by the language client.
    ///
    /// It is recommended that servers register for these file events using the registration
    /// mechanism. This can be done here or in the [`initialized`] method using
    /// `Printer::register_capability()`.
    ///
    /// [`workspace/didChangeWatchedFiles`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#workspace_didChangeConfiguration
    /// [`initialized`]: #tymethod.initialized
    fn did_change_watched_files(&self, printer: &Printer, params: DidChangeWatchedFilesParams) {
        let _ = printer;
        let _ = params;
    }

    /// The [`workspace/symbol`] request is sent from the client to the server to list project-wide
    /// symbols matching the given query string.
    ///
    /// [`workspace/symbol`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#workspace_symbol
    fn symbol(&self, params: WorkspaceSymbolParams) -> Self::SymbolFuture;

    /// The [`workspace/executeCommand`] request is sent from the client to the server to trigger
    /// command execution on the server.
    ///
    /// In most cases, the server creates a `WorkspaceEdit` structure and applies the changes to
    /// the workspace using `Printer::apply_edit()` before returning from this function.
    ///
    /// [`workspace/executeCommand`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#workspace_executeCommand
    fn execute_command(&self, p: &Printer, params: ExecuteCommandParams) -> Self::ExecuteFuture;

    /// The [`textDocument/didOpen`] notification is sent from the client to the server to signal
    /// that a new text document has been opened by the client.
    ///
    /// The document's truth is now managed by the client and the server must not try to read the
    /// document’s truth using the document's URI. "Open" in this sense means it is managed by the
    /// client. It doesn't necessarily mean that its content is presented in an editor.
    ///
    /// [`textDocument/didOpen`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#textDocument_didOpen
    fn did_open(&self, printer: &Printer, params: DidOpenTextDocumentParams) {
        let _ = printer;
        let _ = params;
    }

    /// The [`textDocument/didChange`] notification is sent from the client to the server to signal
    /// changes to a text document.
    ///
    /// This notification will contain a distinct version tag and a list of edits made to the
    /// document for the server to interpret.
    ///
    /// [`textDocument/didChange`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#textDocument_didChange
    fn did_change(&self, printer: &Printer, params: DidChangeTextDocumentParams) {
        let _ = printer;
        let _ = params;
    }

    /// The [`textDocument/didSave`] notification is sent from the client to the server when the
    /// document was saved in the client.
    ///
    /// [`textDocument/didSave`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#textDocument_didSave
    fn did_save(&self, printer: &Printer, params: DidSaveTextDocumentParams) {
        let _ = printer;
        let _ = params;
    }

    /// The [`textDocument/didClose`] notification is sent from the client to the server when the
    /// document got closed in the client.
    ///
    /// The document's truth now exists where the document's URI points to (e.g. if the document's
    /// URI is a file URI, the truth now exists on disk).
    ///
    /// [`textDocument/didClose`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#textDocument_didClose
    fn did_close(&self, printer: &Printer, params: DidCloseTextDocumentParams) {
        let _ = printer;
        let _ = params;
    }

    /// The [`textDocument/completion`] request is sent from the client to the server to compute
    /// completion items at a given cursor position.
    ///
    /// If computing full completion items is expensive, servers can additionally provide a handler
    /// for the completion item resolve request (`completionItem/resolve`). This request is sent
    /// when a completion item is selected in the user interface.
    ///
    /// [`textDocument/completion`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#textDocument_completion
    fn completion(&self, params: CompletionParams) -> Self::CompletionFuture;

    /// The [`textDocument/hover`] request asks the server for hover information at a given text
    /// document position.
    ///
    /// Such hover information typically includes type signature information and inline
    /// documentation for the symbol at the given text document position.
    ///
    /// [`textDocument/hover`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#textDocument_hover
    fn hover(&self, params: TextDocumentPositionParams) -> Self::HoverFuture;

    /// The [`textDocument/documentHighlight`] request is sent from the client to the server to
    /// resolve appropriate highlights for a given text document position.
    ///
    /// For programming languages, this usually highlights all textual references to the symbol
    /// scoped to this file.
    ///
    /// This request differs slightly from `textDocument/references` in that this one is allowed to
    /// be more fuzzy.
    ///
    /// [`textDocument/documentHighlight`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#textDocument_documentHighlight
    fn document_highlight(&self, params: TextDocumentPositionParams) -> Self::HighlightFuture;
}

impl<S: ?Sized + LanguageServer> LanguageServer for Box<S> {
    type ShutdownFuture = S::ShutdownFuture;
    type SymbolFuture = S::SymbolFuture;
    type ExecuteFuture = S::ExecuteFuture;
    type CompletionFuture = S::CompletionFuture;
    type HoverFuture = S::HoverFuture;
    type HighlightFuture = S::HighlightFuture;

    fn initialize(&self, printer: &Printer, params: InitializeParams) -> Result<InitializeResult> {
        (**self).initialize(printer, params)
    }

    fn initialized(&self, printer: &Printer, params: InitializedParams) {
        (**self).initialized(printer, params);
    }

    fn shutdown(&self) -> Self::ShutdownFuture {
        (**self).shutdown()
    }

    fn did_change_workspace_folders(&self, p: &Printer, params: DidChangeWorkspaceFoldersParams) {
        (**self).did_change_workspace_folders(p, params);
    }

    fn did_change_configuration(&self, printer: &Printer, params: DidChangeConfigurationParams) {
        (**self).did_change_configuration(printer, params);
    }

    fn did_change_watched_files(&self, printer: &Printer, params: DidChangeWatchedFilesParams) {
        (**self).did_change_watched_files(printer, params);
    }

    fn symbol(&self, params: WorkspaceSymbolParams) -> Self::SymbolFuture {
        (**self).symbol(params)
    }

    fn execute_command(&self, p: &Printer, params: ExecuteCommandParams) -> Self::ExecuteFuture {
        (**self).execute_command(p, params)
    }

    fn completion(&self, params: CompletionParams) -> Self::CompletionFuture {
        (**self).completion(params)
    }

    fn did_open(&self, printer: &Printer, params: DidOpenTextDocumentParams) {
        (**self).did_open(printer, params);
    }

    fn did_change(&self, printer: &Printer, params: DidChangeTextDocumentParams) {
        (**self).did_change(printer, params);
    }

    fn did_save(&self, printer: &Printer, params: DidSaveTextDocumentParams) {
        (**self).did_save(printer, params);
    }

    fn did_close(&self, printer: &Printer, params: DidCloseTextDocumentParams) {
        (**self).did_close(printer, params);
    }

    fn hover(&self, params: TextDocumentPositionParams) -> Self::HoverFuture {
        (**self).hover(params)
    }

    fn document_highlight(&self, params: TextDocumentPositionParams) -> Self::HighlightFuture {
        (**self).document_highlight(params)
    }
}
