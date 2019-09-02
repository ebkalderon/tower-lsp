//! Language Server Protocol (LSP) server abstraction for [Tower].
//!
//! [Tower]: https://github.com/tower-rs/tower
//!
//! # Example
//!
//! ```rust
//! # use futures::future;
//! # use jsonrpc_core::{BoxFuture, Result};
//! # use tower_lsp::lsp_types::*;
//! # use tower_lsp::{LanguageServer, LspService, Printer, Server};
//! #
//! #[derive(Debug, Default)]
//! struct Backend;
//!
//! impl LanguageServer for Backend {
//!     type ShutdownFuture = BoxFuture<()>;
//!     type HighlightFuture = BoxFuture<Option<Vec<DocumentHighlight>>>;
//!     type HoverFuture = BoxFuture<Option<Hover>>;
//!
//!     fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
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
//!     fn did_open(&self, printer: &Printer, _: DidOpenTextDocumentParams) {
//!         printer.log_message(MessageType::Info, "file opened!");
//!     }
//!
//!     fn did_change(&self, printer: &Printer, _: DidChangeTextDocumentParams) {
//!         printer.log_message(MessageType::Info, "file changed!");
//!     }
//!
//!     fn did_save(&self, printer: &Printer, _: DidSaveTextDocumentParams) {
//!         printer.log_message(MessageType::Info, "file saved!");
//!     }
//!
//!     fn did_close(&self, printer: &Printer, _: DidCloseTextDocumentParams) {
//!         printer.log_message(MessageType::Info, "file closed!");
//!     }
//!
//!     fn hover(&self, _: TextDocumentPositionParams) -> Self::HoverFuture {
//!         Box::new(future::ok(None))
//!     }
//!
//!     fn highlight(&self, _: TextDocumentPositionParams) -> Self::HighlightFuture {
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
#![forbid(unsafe_code)]

pub extern crate lsp_types;

pub use self::delegate::{MessageStream, Printer};
pub use self::service::{ExitReceiver, LspService};
pub use self::stdio::Server;

use futures::Future;
use jsonrpc_core::{Error, Result};
use lsp_types::*;

mod codec;
mod delegate;
mod service;
mod stdio;

/// Trait implemented by language server backends.
///
/// This interface allows servers adhering to the [Language Server Protocol] to be implemented in a
/// safe and easily testable way without exposing the low-level implementation details.
///
/// [Language Server Protocol]: https://microsoft.github.io/language-server-protocol/
pub trait LanguageServer: Send + Sync + 'static {
    type ShutdownFuture: Future<Item = (), Error = Error> + Send;
    type HighlightFuture: Future<Item = Option<Vec<DocumentHighlight>>, Error = Error> + Send;
    type HoverFuture: Future<Item = Option<Hover>, Error = Error> + Send;

    /// The [`initialize`] request is the first request sent from the client to the server.
    ///
    /// [`initialize`]: https://microsoft.github.io/language-server-protocol/specification#initialize
    fn initialize(&self, params: InitializeParams) -> Result<InitializeResult>;

    /// The [`initialized`] notification is sent from the client to the server after the client
    /// received the result of the initialize request but before the client sends anything else.
    ///
    /// The server can use the `initialized` notification for example to dynamically register
    /// capabilities with the client.
    ///
    /// [`initialized`]: https://microsoft.github.io/language-server-protocol/specification#initialized
    fn initialized(&self, printer: &Printer, params: InitializedParams);

    /// The [`shutdown`] request asks the server to gracefully shut down, but to not exit.
    ///
    /// This request is often later followed by an [`exit`] notification, which will cause the
    /// server to exit immediately.
    ///
    /// [`shutdown`]: https://microsoft.github.io/language-server-protocol/specification#shutdown
    /// [`exit`]: https://microsoft.github.io/language-server-protocol/specification#exit
    fn shutdown(&self) -> Self::ShutdownFuture;

    /// The [`textDocument/didOpen`] notification is sent from the client to the server to signal
    /// that a new text document has been opened by the client.
    ///
    /// The document's truth is now managed by the client and the server must not try to read the
    /// document’s truth using the document's URI. "Open" in this sense means it is managed by the
    /// client. It doesn't necessarily mean that its content is presented in an editor.
    ///
    /// [`textDocument/didOpen`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_didOpen
    fn did_open(&self, printer: &Printer, params: DidOpenTextDocumentParams);

    /// The [`textDocument/didOpen`] notification is sent from the client to the server to signal
    /// changes to a text document.
    ///
    /// This notification will contain a distinct version tag and a list of edits made to the
    /// document for the server to interpret.
    ///
    /// [`textDocument/didChange`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_didChange
    fn did_change(&self, printer: &Printer, params: DidChangeTextDocumentParams);

    /// The [`textDocument/didSave`] notification is sent from the client to the server when the
    /// document was saved in the client.
    ///
    /// [`textDocument/didSave`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_didSave
    fn did_save(&self, printer: &Printer, params: DidSaveTextDocumentParams);

    /// The [`textDocument/didClose`] notification is sent from the client to the server when the
    /// document got closed in the client.
    ///
    /// The document's truth now exists where the document's URI points to (e.g. if the document's
    /// URI is a file URI, the truth now exists on disk).
    ///
    /// [`textDocument/didClose`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_didClose
    fn did_close(&self, printer: &Printer, params: DidCloseTextDocumentParams);

    /// The [`textDocument/hover`] request asks the server for hover information at a given text
    /// document position.
    ///
    /// Such hover information typically includes type signature information and inline
    /// documentation for the symbol at the given text document position.
    ///
    /// [`textDocument/hover`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_hover
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
    /// [`textDocument/documentHighlight`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_documentHighlight
    fn highlight(&self, params: TextDocumentPositionParams) -> Self::HighlightFuture;
}

impl<S: ?Sized + LanguageServer> LanguageServer for Box<S> {
    type ShutdownFuture = S::ShutdownFuture;
    type HighlightFuture = S::HighlightFuture;
    type HoverFuture = S::HoverFuture;

    fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        (**self).initialize(params)
    }

    fn initialized(&self, printer: &Printer, params: InitializedParams) {
        (**self).initialized(printer, params);
    }

    fn shutdown(&self) -> Self::ShutdownFuture {
        (**self).shutdown()
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

    fn highlight(&self, params: TextDocumentPositionParams) -> Self::HighlightFuture {
        (**self).highlight(params)
    }
}
