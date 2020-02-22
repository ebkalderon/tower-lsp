//! Language Server Protocol (LSP) server abstraction for [Tower].
//!
//! [Tower]: https://github.com/tower-rs/tower
//!
//! # Example
//!
//! ```rust
//! # use std::future::Future;
//! #
//! # use jsonrpc_core::Result;
//! # use serde_json::Value;
//! # use tower_lsp::lsp_types::request::{GotoDefinitionResponse, GotoImplementationResponse};
//! # use tower_lsp::lsp_types::*;
//! # use tower_lsp::{LanguageServer, LspService, Printer, Server};
//! #
//! #[derive(Debug, Default)]
//! struct Backend;
//!
//! #[tower_lsp::async_trait]
//! impl LanguageServer for Backend {
//!     fn initialize(&self, _: &Printer, _: InitializeParams) -> Result<InitializeResult> {
//!         Ok(InitializeResult::default())
//!     }
//!
//!     fn initialized(&self, printer: &Printer, _: InitializedParams) {
//!         printer.log_message(MessageType::Info, "server initialized!");
//!     }
//!
//!     async fn shutdown(&self) -> Result<()> {
//!         Ok(())
//!     }
//!
//!     async fn symbol(&self, _: WorkspaceSymbolParams) -> Result<Option<Vec<SymbolInformation>>> {
//!         Ok(None)
//!     }
//!
//!     async fn execute_command(&self, _: &Printer, _: ExecuteCommandParams) -> Result<Option<Value>> {
//!         Ok(None)
//!     }
//!
//!     async fn completion(&self, _: CompletionParams) -> Result<Option<CompletionResponse>> {
//!         Ok(None)
//!     }
//!
//!     async fn hover(&self, _: TextDocumentPositionParams) -> Result<Option<Hover>> {
//!         Ok(None)
//!     }
//!
//!     async fn signature_help(&self, _: TextDocumentPositionParams) -> Result<Option<SignatureHelp>> {
//!         Ok(None)
//!     }
//!
//!     async fn goto_declaration(&self, _: TextDocumentPositionParams) -> Result<Option<GotoDefinitionResponse>> {
//!         Ok(None)
//!     }
//!
//!     async fn goto_definition(&self, _: TextDocumentPositionParams) -> Result<Option<GotoDefinitionResponse>> {
//!         Ok(None)
//!     }
//!
//!     async fn goto_type_definition(&self, _: TextDocumentPositionParams) -> Result<Option<GotoDefinitionResponse>> {
//!         Ok(None)
//!     }
//!
//!     async fn goto_implementation(&self, _: TextDocumentPositionParams) -> Result<Option<GotoImplementationResponse>> {
//!         Ok(None)
//!     }
//!
//!     async fn document_highlight(&self, _: TextDocumentPositionParams) -> Result<Option<Vec<DocumentHighlight>>> {
//!         Ok(None)
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let stdin = tokio::io::stdin();
//!     let stdout = tokio::io::stdout();
//!
//!     let (service, messages) = LspService::new(Backend::default());
//!     let handle = service.close_handle();
//!     let server = Server::new(stdin, stdout)
//!         .interleave(messages)
//!         .serve(service);
//!
//!     handle.run_until_exit(server).await;
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
/// A re-export of [`async-trait`](https://docs.rs/async-trait) for convenience.
pub use async_trait::async_trait;

use jsonrpc_core::Result;
use lsp_types::request::{GotoDefinitionResponse, GotoImplementationResponse};
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
#[async_trait]
pub trait LanguageServer: Send + Sync + 'static {
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
    async fn shutdown(&self) -> Result<()>;

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
    async fn symbol(&self, params: WorkspaceSymbolParams)
        -> Result<Option<Vec<SymbolInformation>>>;

    /// The [`workspace/executeCommand`] request is sent from the client to the server to trigger
    /// command execution on the server.
    ///
    /// In most cases, the server creates a `WorkspaceEdit` structure and applies the changes to
    /// the workspace using `Printer::apply_edit()` before returning from this function.
    ///
    /// [`workspace/executeCommand`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#workspace_executeCommand
    async fn execute_command(
        &self,
        p: &Printer,
        params: ExecuteCommandParams,
    ) -> Result<Option<Value>>;

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
    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>>;

    /// The [`textDocument/hover`] request asks the server for hover information at a given text
    /// document position.
    ///
    /// Such hover information typically includes type signature information and inline
    /// documentation for the symbol at the given text document position.
    ///
    /// [`textDocument/hover`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#textDocument_hover
    async fn hover(&self, params: TextDocumentPositionParams) -> Result<Option<Hover>>;

    /// The [`textDocument/signatureHelp`] request is sent from the client to the server to request
    /// signature information at a given cursor position.
    ///
    /// [`textDocument/signatureHelp`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#textDocument_signatureHelp
    async fn signature_help(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<SignatureHelp>>;

    /// The [`textDocument/declaration`] request asks the server for the declaration location of a
    /// symbol at a given text document position.
    ///
    /// The [`GotoDefinitionResponse::Link`] return value was introduced in specification version
    /// 3.14.0 and requires client-side support. It can be returned if the client set the following
    /// field to `true` in the [`initialize`] method:
    ///
    /// ```text
    /// InitializeParams::capabilities::text_document::declaration::link_support
    /// ```
    ///
    /// [`textDocument/declaration`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#textDocument_declaration
    /// [`GotoDefinitionResponse::Link`]: https://docs.rs/lsp-types/0.63.1/lsp_types/request/enum.GotoDefinitionResponse.html#variant.Link
    /// [`initialize`]: #tymethod.initialize
    async fn goto_declaration(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<GotoDefinitionResponse>>;

    /// The [`textDocument/definition`] request asks the server for the definition location of a
    /// symbol at a given text document position.
    ///
    /// The [`GotoDefinitionResponse::Link`] return value was introduced in specification version
    /// 3.14.0 and requires client-side support. It can be returned if the client set the following
    /// field to `true` in the [`initialize`] method:
    ///
    /// ```text
    /// InitializeParams::capabilities::text_document::definition::link_support
    /// ```
    ///
    /// [`textDocument/definition`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#textDocument_definition
    /// [`GotoDefinitionResponse::Link`]: https://docs.rs/lsp-types/0.63.1/lsp_types/request/enum.GotoDefinitionResponse.html#variant.Link
    /// [`initialize`]: #tymethod.initialize
    async fn goto_definition(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<GotoDefinitionResponse>>;

    /// The [`textDocument/typeDefinition`] request asks the server for the type definition location of
    /// a symbol at a given text document position.
    ///
    /// The [`GotoDefinitionResponse::Link`] return value was introduced in specification version
    /// 3.14.0 and requires client-side support. It can be returned if the client set the following
    /// field to `true` in the [`initialize`] method:
    ///
    /// ```text
    /// InitializeParams::capabilities::text_document::type_definition::link_support
    /// ```
    ///
    /// [`textDocument/typeDefinition`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#textDocument_typeDefinition
    /// [`GotoDefinitionResponse::Link`]: https://docs.rs/lsp-types/0.63.1/lsp_types/request/enum.GotoDefinitionResponse.html#variant.Link
    /// [`initialize`]: #tymethod.initialize
    async fn goto_type_definition(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<GotoDefinitionResponse>>;

    /// The [`textDocument/implementation`] request is sent from the client to the server to resolve
    /// the implementation location of a symbol at a given text document position.
    ///
    /// The result type [`GotoImplementationResponse::Link`] got introduced with version 3.14.0 and
    /// requires client-side support. It can be returned if the client set the following
    /// field to `true` in the [`initialize`] method:
    ///
    /// ```text
    /// InitializeParams::capabilities::text_document::implementation::link_support
    /// ```
    ///
    /// [`textDocument/implementation`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#textDocument_implementation
    /// [`GotoImplementationResponse::Link`]: https://docs.rs/lsp-types/0.63.1/lsp_types/request/enum.GotoDefinitionResponse.html
    /// [`initialize`]: #tymethod.initialize
    async fn goto_implementation(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<GotoImplementationResponse>>;

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
    async fn document_highlight(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<Vec<DocumentHighlight>>>;
}

#[async_trait]
impl<S: ?Sized + LanguageServer> LanguageServer for Box<S> {
    fn initialize(&self, printer: &Printer, params: InitializeParams) -> Result<InitializeResult> {
        (**self).initialize(printer, params)
    }

    fn initialized(&self, printer: &Printer, params: InitializedParams) {
        (**self).initialized(printer, params);
    }

    async fn shutdown(&self) -> Result<()> {
        (**self).shutdown().await
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

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        (**self).symbol(params).await
    }

    async fn execute_command(
        &self,
        p: &Printer,
        params: ExecuteCommandParams,
    ) -> Result<Option<Value>> {
        (**self).execute_command(p, params).await
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

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        (**self).completion(params).await
    }

    async fn hover(&self, params: TextDocumentPositionParams) -> Result<Option<Hover>> {
        (**self).hover(params).await
    }

    async fn signature_help(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<SignatureHelp>> {
        (**self).signature_help(params).await
    }

    async fn goto_declaration(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        (**self).goto_declaration(params).await
    }

    async fn goto_definition(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        (**self).goto_definition(params).await
    }

    async fn goto_type_definition(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        (**self).goto_type_definition(params).await
    }

    async fn goto_implementation(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<GotoImplementationResponse>> {
        (**self).goto_implementation(params).await
    }

    async fn document_highlight(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        (**self).document_highlight(params).await
    }
}
