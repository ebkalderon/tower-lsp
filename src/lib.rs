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
//!     async fn completion(&self, _: CompletionParams) -> Result<Option<CompletionResponse>> {
//!         Ok(Some(CompletionResponse::Array(vec![
//!             CompletionItem::new_simple("Hello".to_string(), "Some detail".to_string()),
//!             CompletionItem::new_simple("Bye".to_string(), "More detail".to_string())
//!         ])))
//!     }
//!
//!     async fn hover(&self, _: TextDocumentPositionParams) -> Result<Option<Hover>> {
//!         Ok(Some(Hover {
//!             contents: HoverContents::Scalar(
//!                 MarkedString::String("You're hovering!".to_string())
//!             ),
//!             range: None
//!         }))
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let stdin = tokio::io::stdin();
//!     let stdout = tokio::io::stdout();
//!
//!     let (service, messages) = LspService::new(Backend::default());
//!     Server::new(stdin, stdout)
//!         .interleave(messages)
//!         .serve(service)
//!         .await;
//! }
//! ```

#![deny(missing_debug_implementations)]
#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub extern crate lsp_types;

pub use self::delegate::{MessageStream, Printer};
pub use self::message::Incoming;
pub use self::service::{ExitedError, LspService};
pub use self::stdio::Server;
/// A re-export of [`async-trait`](https://docs.rs/async-trait) for convenience.
pub use async_trait::async_trait;

use jsonrpc_core::{Error, Result};
use log::{error, warn};
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
        warn!("Got a workspace/didChangeWorkspaceFolders notification, but it is not implemented");
    }

    /// The [`workspace/didChangeConfiguration`] notification is sent from the client to the server
    /// to signal the change of configuration settings.
    ///
    /// [`workspace/didChangeConfiguration`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#workspace_didChangeConfiguration
    fn did_change_configuration(&self, printer: &Printer, params: DidChangeConfigurationParams) {
        let _ = printer;
        let _ = params;
        warn!("Got a workspace/didChangeConfiguration notification, but it is not implemented");
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
        warn!("Got a workspace/didChangeWatchedFiles notification, but it is not implemented");
    }

    /// The [`workspace/symbol`] request is sent from the client to the server to list project-wide
    /// symbols matching the given query string.
    ///
    /// [`workspace/symbol`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#workspace_symbol
    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let _ = params;
        error!("Got a workspace/symbol request, but it is not implemented");
        Err(Error::method_not_found())
    }

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
    ) -> Result<Option<Value>> {
        let _ = p;
        let _ = params;
        error!("Got a workspace/executeCommand request, but it is not implemented");
        Err(Error::method_not_found())
    }

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
        warn!("Got a textDocument/didOpen notification, but it is not implemented");
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
        warn!("Got a textDocument/didChange notification, but it is not implemented");
    }

    /// The [`textDocument/willSave`] notification is sent from the client to the server before the
    /// document is actually saved.
    ///
    /// [`textDocument/willSave`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#textDocument_willSave
    fn will_save(&self, printer: &Printer, params: WillSaveTextDocumentParams) {
        let _ = printer;
        let _ = params;
        warn!("Got a textDocument/willSave notification, but it is not implemented");
    }

    /// The [`textDocument/didSave`] notification is sent from the client to the server when the
    /// document was saved in the client.
    ///
    /// [`textDocument/didSave`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#textDocument_didSave
    fn did_save(&self, printer: &Printer, params: DidSaveTextDocumentParams) {
        let _ = printer;
        let _ = params;
        warn!("Got a textDocument/didSave notification, but it is not implemented");
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
        warn!("Got a textDocument/didClose notification, but it is not implemented");
    }

    /// The [`textDocument/completion`] request is sent from the client to the server to compute
    /// completion items at a given cursor position.
    ///
    /// If computing full completion items is expensive, servers can additionally provide a handler
    /// for the completion item resolve request (`completionItem/resolve`). This request is sent
    /// when a completion item is selected in the user interface.
    ///
    /// [`textDocument/completion`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#textDocument_completion
    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let _ = params;
        error!("Got a textDocument/completion request, but it is not implemented");
        Err(Error::method_not_found())
    }

    /// The [`completionItem/resolve`] request is sent from the client to the server to resolve
    /// additional information for a given completion item.
    ///
    /// [`completionItem/resolve`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#completionItem_resolve
    async fn completion_resolve(&self, params: CompletionItem) -> Result<CompletionItem> {
        let _ = params;
        error!("Got a completionItem/resolve request, but it is not implemented");
        Err(Error::method_not_found())
    }

    /// The [`textDocument/hover`] request asks the server for hover information at a given text
    /// document position.
    ///
    /// Such hover information typically includes type signature information and inline
    /// documentation for the symbol at the given text document position.
    ///
    /// [`textDocument/hover`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#textDocument_hover
    async fn hover(&self, params: TextDocumentPositionParams) -> Result<Option<Hover>> {
        let _ = params;
        error!("Got a textDocument/hover request, but it is not implemented");
        Err(Error::method_not_found())
    }

    /// The [`textDocument/signatureHelp`] request is sent from the client to the server to request
    /// signature information at a given cursor position.
    ///
    /// [`textDocument/signatureHelp`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#textDocument_signatureHelp
    async fn signature_help(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<SignatureHelp>> {
        let _ = params;
        error!("Got a textDocument/signatureHelp request, but it is not implemented");
        Err(Error::method_not_found())
    }

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
    ) -> Result<Option<GotoDefinitionResponse>> {
        let _ = params;
        error!("Got a textDocument/declaration request, but it is not implemented");
        Err(Error::method_not_found())
    }

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
    ) -> Result<Option<GotoDefinitionResponse>> {
        let _ = params;
        error!("Got a textDocument/definition request, but it is not implemented");
        Err(Error::method_not_found())
    }

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
    ) -> Result<Option<GotoDefinitionResponse>> {
        let _ = params;
        error!("Got a textDocument/typeDefinition request, but it is not implemented");
        Err(Error::method_not_found())
    }

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
    ) -> Result<Option<GotoImplementationResponse>> {
        let _ = params;
        error!("Got a textDocument/implementation request, but it is not implemented");
        Err(Error::method_not_found())
    }

    /// The [`textDocument/documentSymbol`] request is sent from the client to the server to
    /// retrieve all symbols found in a given text document.
    ///
    /// The returned result is either:
    ///
    /// * [`DocumentSymbolResponse::Flat`] which is a flat list of all symbols found in a given
    ///   text document. Then neither the symbol’s location range nor the symbol’s container name
    ///   should be used to infer a hierarchy.
    /// * [`DocumentSymbolResponse::Nested`] which is a hierarchy of symbols found in a given text
    ///   document.
    ///
    /// [`textDocument/documentSymbol`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#textDocument_documentSymbol
    /// [`DocumentSymbolResponse::Flat`]: https://docs.rs/lsp-types/0.70.2/lsp_types/enum.DocumentSymbolResponse.html#variant.Flat
    /// [`DocumentSymbolResponse::Nested`]: https://docs.rs/lsp-types/0.70.2/lsp_types/enum.DocumentSymbolResponse.html#variant.Nested
    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let _ = params;
        error!("Got a textDocument/documentSymbol request, but it is not implemented");
        Err(Error::method_not_found())
    }

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
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        let _ = params;
        error!("Got a textDocument/documentHighlight request, but it is not implemented");
        Err(Error::method_not_found())
    }

    /// The [`textDocument/codeAction`] request is sent from the client to the server to compute
    /// commands for a given text document and range. These commands are typically code fixes to
    /// either fix problems or to beautify/refactor code. The result of a [`textDocument/codeAction`]
    /// request is an array of `Command` literals which are typically presented in the user interface.
    /// To ensure that a server is useful in many clients the commands specified in a code actions
    /// should be handled by the server and not by the client (see [`workspace/executeCommand`] and
    /// `ServerCapabilities::execute_command_provider`). If the client supports providing edits
    /// with a code action then the mode should be used.
    ///
    /// When the command is selected the server should be contacted again
    /// (via the [`workspace/executeCommand`]) request to execute the command.
    ///
    /// Since version 3.8.0: support for `CodeAction` literals to enable the following scenarios:
    ///
    /// - the ability to directly return a workspace edit from the code action request.
    /// This avoids having another server roundtrip to execute an actual code action.
    /// However server providers should be aware that if the code action is expensive to compute or
    /// the edits are huge it might still be beneficial if the result is simply a command and the
    /// actual edit is only computed when needed.
    ///
    /// - the ability to group code actions using a kind. Clients are allowed to ignore that
    /// information. However it allows them to better group code action for example into
    /// corresponding menus (e.g. all refactor code actions into a refactor menu).
    ///
    /// [`textDocument/codeAction`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#textDocument_codeAction
    /// [`workspace/executeCommand`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#workspace_executeCommand
    ///
    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let _ = params;
        error!("Got a textDocument/codeAction request, but it is not implemented");
        Err(Error::method_not_found())
    }

    /// The [`textDocument/codeLens`] request is sent from the client to the server to compute code
    /// lenses for a given text document.
    ///
    /// [`textDocument/codeLens`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#textDocument_codeLens
    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let _ = params;
        error!("Got a textDocument/codeLens request, but it is not implemented");
        Err(Error::method_not_found())
    }

    /// The [`codeLens/resolve`] request is sent from the client to the server to resolve the
    /// command for a given code lens item.
    ///
    /// [`codeLens/resolve`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-15/#codeLens_resolve
    async fn code_lens_resolve(&self, params: CodeLens) -> Result<CodeLens> {
        let _ = params;
        error!("Got a codeLens/resolve request, but it is not implemented");
        Err(Error::method_not_found())
    }
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

    fn will_save(&self, printer: &Printer, params: WillSaveTextDocumentParams) {
        (**self).will_save(printer, params);
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

    async fn completion_resolve(&self, params: CompletionItem) -> Result<CompletionItem> {
        (**self).completion_resolve(params).await
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

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        (**self).document_symbol(params).await
    }

    async fn document_highlight(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        (**self).document_highlight(params).await
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        (**self).code_action(params).await
    }

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        (**self).code_lens(params).await
    }

    async fn code_lens_resolve(&self, params: CodeLens) -> Result<CodeLens> {
        (**self).code_lens_resolve(params).await
    }
}
