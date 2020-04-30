//! Language Server Protocol (LSP) server abstraction for [Tower].
//!
//! [Tower]: https://github.com/tower-rs/tower
//!
//! # Example
//!
//! ```rust
//! use tower_lsp::jsonrpc::Result;
//! use tower_lsp::lsp_types::*;
//! use tower_lsp::{LanguageServer, LspService, Client, Server};
//!
//! #[derive(Debug, Default)]
//! struct Backend;
//!
//! #[tower_lsp::async_trait]
//! impl LanguageServer for Backend {
//!     async fn initialize(&self, _: &Client, _: InitializeParams) -> Result<InitializeResult> {
//!         Ok(InitializeResult::default())
//!     }
//!
//!     async fn initialized(&self, client: &Client, _: InitializedParams) {
//!         client.log_message(MessageType::Info, "server initialized!");
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
//!     async fn hover(&self, _: HoverParams) -> Result<Option<Hover>> {
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
//! #   use std::io::Cursor;
//!     let stdin = tokio::io::stdin();
//! #   let message = r#"{"jsonrpc":"2.0","method":"exit"}"#;
//! #   let stdin = Cursor::new(format!("Content-Length: {}\r\n\r\n{}", message.len(), message).into_bytes());
//!     let stdout = tokio::io::stdout();
//! #   let stdout = Cursor::new(Vec::new());
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

pub use self::delegate::{Client, MessageStream};
pub use self::message::Incoming;
pub use self::service::{ExitedError, LspService};
pub use self::stdio::Server;
/// A re-export of [`async-trait`](https://docs.rs/async-trait) for convenience.
pub use async_trait::async_trait;

use jsonrpc_core::{Error, Result};
use log::{error, warn};
use lsp_types::request::{
    GotoDeclarationParams, GotoDeclarationResponse, GotoImplementationParams,
    GotoImplementationResponse, GotoTypeDefinitionParams, GotoTypeDefinitionResponse,
};
use lsp_types::*;
use serde_json::Value;

pub mod jsonrpc;

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
    /// [`initialize`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#initialize
    ///
    /// This method is guaranteed to only execute once. If the client sends this request to the
    /// server again, the server will respond with JSON-RPC error code `-32600` (invalid request).
    async fn initialize(
        &self,
        client: &Client,
        params: InitializeParams,
    ) -> Result<InitializeResult>;

    /// The [`initialized`] notification is sent from the client to the server after the client
    /// received the result of the initialize request but before the client sends anything else.
    ///
    /// The server can use the `initialized` notification for example to dynamically register
    /// capabilities with the client.
    ///
    /// [`initialized`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#initialized
    async fn initialized(&self, client: &Client, params: InitializedParams) {
        let _ = client;
        let _ = params;
    }

    /// The [`shutdown`] request asks the server to gracefully shut down, but to not exit.
    ///
    /// This request is often later followed by an [`exit`] notification, which will cause the
    /// server to exit immediately.
    ///
    /// [`shutdown`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#shutdown
    /// [`exit`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#exit
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
    /// [`workspace/didChangeWorkspaceFolders`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#workspace_didChangeWorkspaceFolders
    /// [`initialize`]: #tymethod.initialize
    async fn did_change_workspace_folders(
        &self,
        client: &Client,
        params: DidChangeWorkspaceFoldersParams,
    ) {
        let _ = client;
        let _ = params;
        warn!("Got a workspace/didChangeWorkspaceFolders notification, but it is not implemented");
    }

    /// The [`workspace/didChangeConfiguration`] notification is sent from the client to the server
    /// to signal the change of configuration settings.
    ///
    /// [`workspace/didChangeConfiguration`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#workspace_didChangeConfiguration
    async fn did_change_configuration(
        &self,
        client: &Client,
        params: DidChangeConfigurationParams,
    ) {
        let _ = client;
        let _ = params;
        warn!("Got a workspace/didChangeConfiguration notification, but it is not implemented");
    }

    /// The [`workspace/didChangeWatchedFiles`] notification is sent from the client to the server
    /// when the client detects changes to files watched by the language client.
    ///
    /// It is recommended that servers register for these file events using the registration
    /// mechanism. This can be done here or in the [`initialized`] method using
    /// `Client::register_capability()`.
    ///
    /// [`workspace/didChangeWatchedFiles`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#workspace_didChangeConfiguration
    /// [`initialized`]: #tymethod.initialized
    async fn did_change_watched_files(&self, client: &Client, params: DidChangeWatchedFilesParams) {
        let _ = client;
        let _ = params;
        warn!("Got a workspace/didChangeWatchedFiles notification, but it is not implemented");
    }

    /// The [`workspace/symbol`] request is sent from the client to the server to list project-wide
    /// symbols matching the given query string.
    ///
    /// [`workspace/symbol`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#workspace_symbol
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
    /// the workspace using `Client::apply_edit()` before returning from this function.
    ///
    /// [`workspace/executeCommand`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#workspace_executeCommand
    async fn execute_command(
        &self,
        client: &Client,
        params: ExecuteCommandParams,
    ) -> Result<Option<Value>> {
        let _ = client;
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
    /// [`textDocument/didOpen`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#textDocument_didOpen
    async fn did_open(&self, client: &Client, params: DidOpenTextDocumentParams) {
        let _ = client;
        let _ = params;
        warn!("Got a textDocument/didOpen notification, but it is not implemented");
    }

    /// The [`textDocument/didChange`] notification is sent from the client to the server to signal
    /// changes to a text document.
    ///
    /// This notification will contain a distinct version tag and a list of edits made to the
    /// document for the server to interpret.
    ///
    /// [`textDocument/didChange`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#textDocument_didChange
    async fn did_change(&self, client: &Client, params: DidChangeTextDocumentParams) {
        let _ = client;
        let _ = params;
        warn!("Got a textDocument/didChange notification, but it is not implemented");
    }

    /// The [`textDocument/willSave`] notification is sent from the client to the server before the
    /// document is actually saved.
    ///
    /// [`textDocument/willSave`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#textDocument_willSave
    async fn will_save(&self, client: &Client, params: WillSaveTextDocumentParams) {
        let _ = client;
        let _ = params;
        warn!("Got a textDocument/willSave notification, but it is not implemented");
    }

    /// The [`textDocument/willSaveWaitUntil`] request is sent from the client to the server before
    /// the document is actually saved.
    ///
    /// The request can return an array of `TextEdit`s which will be applied to the text document
    /// before it is saved.
    ///
    /// Please note that clients might drop results if computing the text edits took too long or if
    /// a server constantly fails on this request. This is done to keep the save fast and reliable.
    async fn will_save_wait_until(
        &self,
        params: WillSaveTextDocumentParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let _ = params;
        error!("Got a textDocument/willSaveWaitUntil request, but it is not implemented");
        Err(Error::method_not_found())
    }

    /// The [`textDocument/didSave`] notification is sent from the client to the server when the
    /// document was saved in the client.
    ///
    /// [`textDocument/didSave`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#textDocument_didSave
    async fn did_save(&self, client: &Client, params: DidSaveTextDocumentParams) {
        let _ = client;
        let _ = params;
        warn!("Got a textDocument/didSave notification, but it is not implemented");
    }

    /// The [`textDocument/didClose`] notification is sent from the client to the server when the
    /// document got closed in the client.
    ///
    /// The document's truth now exists where the document's URI points to (e.g. if the document's
    /// URI is a file URI, the truth now exists on disk).
    ///
    /// [`textDocument/didClose`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#textDocument_didClose
    async fn did_close(&self, client: &Client, params: DidCloseTextDocumentParams) {
        let _ = client;
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
    /// [`textDocument/completion`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#textDocument_completion
    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let _ = params;
        error!("Got a textDocument/completion request, but it is not implemented");
        Err(Error::method_not_found())
    }

    /// The [`completionItem/resolve`] request is sent from the client to the server to resolve
    /// additional information for a given completion item.
    ///
    /// [`completionItem/resolve`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#completionItem_resolve
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
    /// [`textDocument/hover`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#textDocument_hover
    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let _ = params;
        error!("Got a textDocument/hover request, but it is not implemented");
        Err(Error::method_not_found())
    }

    /// The [`textDocument/signatureHelp`] request is sent from the client to the server to request
    /// signature information at a given cursor position.
    ///
    /// [`textDocument/signatureHelp`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#textDocument_signatureHelp
    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let _ = params;
        error!("Got a textDocument/signatureHelp request, but it is not implemented");
        Err(Error::method_not_found())
    }

    /// The [`textDocument/declaration`] request asks the server for the declaration location of a
    /// symbol at a given text document position.
    ///
    /// [`textDocument/declaration`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#textDocument_declaration
    ///
    /// # Compatibility
    ///
    /// This request was introduced in specification version 3.14.0.
    ///
    /// The [`GotoDefinitionResponse::Link`] return value was introduced in specification version
    /// 3.14.0 and requires client-side support in order to be used. It can be returned if the
    /// client set the following field to `true` in the [`initialize`] method:
    ///
    /// ```text
    /// InitializeParams::capabilities::text_document::declaration::link_support
    /// ```
    ///
    /// [`GotoDefinitionResponse::Link`]: https://docs.rs/lsp-types/0.74.0/lsp_types/enum.GotoDefinitionResponse.html#variant.Link
    /// [`initialize`]: #tymethod.initialize
    async fn goto_declaration(
        &self,
        params: GotoDeclarationParams,
    ) -> Result<Option<GotoDeclarationResponse>> {
        let _ = params;
        error!("Got a textDocument/declaration request, but it is not implemented");
        Err(Error::method_not_found())
    }

    /// The [`textDocument/definition`] request asks the server for the definition location of a
    /// symbol at a given text document position.
    ///
    /// [`textDocument/definition`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#textDocument_definition
    ///
    /// # Compatibility
    ///
    /// The [`GotoDefinitionResponse::Link`] return value was introduced in specification version
    /// 3.14.0 and requires client-side support in order to be used. It can be returned if the
    /// client set the following field to `true` in the [`initialize`] method:
    ///
    /// ```text
    /// InitializeParams::capabilities::text_document::definition::link_support
    /// ```
    ///
    /// [`GotoDefinitionResponse::Link`]: https://docs.rs/lsp-types/0.74.0/lsp_types/enum.GotoDefinitionResponse.html#variant.Link
    /// [`initialize`]: #tymethod.initialize
    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let _ = params;
        error!("Got a textDocument/definition request, but it is not implemented");
        Err(Error::method_not_found())
    }

    /// The [`textDocument/typeDefinition`] request asks the server for the type definition location of
    /// a symbol at a given text document position.
    ///
    /// [`textDocument/typeDefinition`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#textDocument_typeDefinition
    ///
    /// # Compatibility
    ///
    /// This request was introduced in specification version 3.6.0.
    ///
    /// The [`GotoDefinitionResponse::Link`] return value was introduced in specification version
    /// 3.14.0 and requires client-side support in order to be used. It can be returned if the
    /// client set the following field to `true` in the [`initialize`] method:
    ///
    /// ```text
    /// InitializeParams::capabilities::text_document::type_definition::link_support
    /// ```
    ///
    /// [`GotoDefinitionResponse::Link`]: https://docs.rs/lsp-types/0.74.0/lsp_types/enum.GotoDefinitionResponse.html#variant.Link
    /// [`initialize`]: #tymethod.initialize
    async fn goto_type_definition(
        &self,
        params: GotoTypeDefinitionParams,
    ) -> Result<Option<GotoTypeDefinitionResponse>> {
        let _ = params;
        error!("Got a textDocument/typeDefinition request, but it is not implemented");
        Err(Error::method_not_found())
    }

    /// The [`textDocument/implementation`] request is sent from the client to the server to resolve
    /// the implementation location of a symbol at a given text document position.
    ///
    /// [`textDocument/implementation`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#textDocument_implementation
    ///
    /// # Compatibility
    ///
    /// This request was introduced in specification version 3.6.0.
    ///
    /// The [`GotoImplementationResponse::Link`] return value was introduced in specification
    /// version 3.14.0 and requires client-side support in order to be used. It can be returned if
    /// the client set the following field to `true` in the [`initialize`] method:
    ///
    /// ```text
    /// InitializeParams::capabilities::text_document::implementation::link_support
    /// ```
    ///
    /// [`GotoImplementationResponse::Link`]: https://docs.rs/lsp-types/0.74.0/lsp_types/enum.GotoDefinitionResponse.html#variant.Link
    /// [`initialize`]: #tymethod.initialize
    async fn goto_implementation(
        &self,
        params: GotoImplementationParams,
    ) -> Result<Option<GotoImplementationResponse>> {
        let _ = params;
        error!("Got a textDocument/implementation request, but it is not implemented");
        Err(Error::method_not_found())
    }

    /// The [`textDocument/references`] request is sent from the client to the server to resolve
    /// project-wide references for the symbol denoted by the given text document position.
    ///
    /// [`textDocument/references`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#textDocument_references
    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let _ = params;
        error!("Got a textDocument/references request, but it is not implemented");
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
    /// [`textDocument/documentHighlight`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#textDocument_documentHighlight
    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        let _ = params;
        error!("Got a textDocument/documentHighlight request, but it is not implemented");
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
    /// [`DocumentSymbolResponse::Flat`]: https://docs.rs/lsp-types/0.74.0/lsp_types/enum.DocumentSymbolResponse.html#variant.Flat
    /// [`DocumentSymbolResponse::Nested`]: https://docs.rs/lsp-types/0.74.0/lsp_types/enum.DocumentSymbolResponse.html#variant.Nested
    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let _ = params;
        error!("Got a textDocument/documentSymbol request, but it is not implemented");
        Err(Error::method_not_found())
    }

    /// The [`textDocument/codeAction`] request is sent from the client to the server to compute
    /// commands for a given text document and range. These commands are typically code fixes to
    /// either fix problems or to beautify/refactor code.
    ///
    /// The result of a [`textDocument/codeAction`] request is an array of `Command` literals which
    /// are typically presented in the user interface.
    ///
    /// To ensure that a server is useful in many clients, the commands specified in a code actions
    /// should be handled by the server and not by the client (see [`workspace/executeCommand`] and
    /// `ServerCapabilities::execute_command_provider`). If the client supports providing edits
    /// with a code action, then the mode should be used.
    ///
    /// When the command is selected the server should be contacted again (via the
    /// [`workspace/executeCommand`] request) to execute the command.
    ///
    /// # Compatibility
    ///
    /// Since version 3.8.0: support for `CodeAction` literals to enable the following scenarios:
    ///
    /// * The ability to directly return a workspace edit from the code action request.
    ///   This avoids having another server roundtrip to execute an actual code action.
    ///   However server providers should be aware that if the code action is expensive to compute
    ///   or the edits are huge it might still be beneficial if the result is simply a command and
    ///   the actual edit is only computed when needed.
    ///
    /// * The ability to group code actions using a kind. Clients are allowed to ignore that
    ///   information. However it allows them to better group code action for example into
    ///   corresponding menus (e.g. all refactor code actions into a refactor menu).
    ///
    /// [`textDocument/codeAction`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#textDocument_codeAction
    /// [`workspace/executeCommand`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#workspace_executeCommand
    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let _ = params;
        error!("Got a textDocument/codeAction request, but it is not implemented");
        Err(Error::method_not_found())
    }

    /// The [`textDocument/codeLens`] request is sent from the client to the server to compute code
    /// lenses for a given text document.
    ///
    /// [`textDocument/codeLens`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#textDocument_codeLens
    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let _ = params;
        error!("Got a textDocument/codeLens request, but it is not implemented");
        Err(Error::method_not_found())
    }

    /// The [`codeLens/resolve`] request is sent from the client to the server to resolve the
    /// command for a given code lens item.
    ///
    /// [`codeLens/resolve`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#codeLens_resolve
    async fn code_lens_resolve(&self, params: CodeLens) -> Result<CodeLens> {
        let _ = params;
        error!("Got a codeLens/resolve request, but it is not implemented");
        Err(Error::method_not_found())
    }

    /// The [`textDocument/documentLink`] request is sent from the client to the server to request
    /// the location of links in a document.
    ///
    /// A document link is a range in a text document that links to an internal or external
    /// resource, like another text document or a web site.
    ///
    /// [`textDocument/documentLink`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#textDocument_documentLink
    ///
    /// # Compatibility
    ///
    /// The [`DocumentLink::tooltip`] field was introduced in specification version 3.15.0 and
    /// requires client-side support in order to be used. It can be returned if the client set the
    /// following field to `true` in the [`initialize`] method:
    ///
    /// ```text
    /// InitializeParams::capabilities::text_document::document_link::tooltip_support
    /// ```
    ///
    /// [`initialize`]: #tymethod.initialize
    async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        let _ = params;
        error!("Got a textDocument/documentLink request, but it is not implemented");
        Err(Error::method_not_found())
    }

    /// The [`documentLink/resolve`] request is sent from the client to the server to resolve the
    /// target of a given document link.
    ///
    /// A document link is a range in a text document that links to an internal or external
    /// resource, like another text document or a web site.
    ///
    /// [`documentLink/resolve`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#documentLink_resolve
    async fn document_link_resolve(&self, params: DocumentLink) -> Result<DocumentLink> {
        let _ = params;
        error!("Got a documentLink/resolve request, but it is not implemented");
        Err(Error::method_not_found())
    }

    /// The [`textDocument/documentColor`] request is sent from the client to the server to list
    /// all color references found in a given text document. Along with the range, a color value in
    /// RGB is returned.
    ///
    /// [`textDocument/documentColor`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#textDocument_documentColor
    ///
    /// Clients can use the result to decorate color references in an editor. For example:
    ///
    /// * Color boxes showing the actual color next to the reference
    /// * Show a color picker when a color reference is edited
    ///
    /// # Compatibility
    ///
    /// This request was introduced in specification version 3.6.0.
    async fn document_color(&self, params: DocumentColorParams) -> Result<Vec<ColorInformation>> {
        let _ = params;
        error!("Got a textDocument/documentColor request, but it is not implemented");
        Err(Error::method_not_found())
    }

    /// The [`textDocument/colorPresentation`] request is sent from the client to the server to
    /// obtain a list of presentations for a color value at a given location.
    ///
    /// Clients can use the result to:
    ///
    /// * Modify a color reference
    /// * Show in a color picker and let users pick one of the presentations
    ///
    /// # Compatibility
    ///
    /// This request was introduced in specification version 3.6.0.
    ///
    /// This request has no special capabilities and registration options since it is sent as a
    /// resolve request for the [`textDocument/documentColor`] request.
    ///
    /// [`textDocument/documentColor`]: #tymethod.document_color
    async fn color_presentation(
        &self,
        params: ColorPresentationParams,
    ) -> Result<Vec<ColorPresentation>> {
        let _ = params;
        error!("Got a textDocument/colorPresentation request, but it is not implemented");
        Err(Error::method_not_found())
    }

    /// The [`textDocument/formatting`] request is sent from the client to the server to format a
    /// whole document.
    ///
    /// [`textDocument/formatting`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#textDocument_formatting
    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let _ = params;
        error!("Got a textDocument/formatting request, but it is not implemented");
        Err(Error::method_not_found())
    }

    /// The [`textDocument/rangeFormatting`] request is sent from the client to the server to
    /// format a given range in a document.
    ///
    /// [`textDocument/rangeFormatting`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#textDocument_rangeFormatting
    async fn range_formatting(
        &self,
        params: DocumentRangeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let _ = params;
        error!("Got a textDocument/rangeFormatting request, but it is not implemented");
        Err(Error::method_not_found())
    }

    /// The [`textDocument/onTypeFormatting`] request is sent from the client to the server to
    /// format parts of the document during typing.
    ///
    /// [`textDocument/onTypeFormatting`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#textDocument_onTypeFormatting
    async fn on_type_formatting(
        &self,
        params: DocumentOnTypeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let _ = params;
        error!("Got a textDocument/onTypeFormatting request, but it is not implemented");
        Err(Error::method_not_found())
    }

    /// The [`textDocument/rename`] request is sent from the client to the server to ask the server
    /// to compute a workspace change so that the client can perform a workspace-wide rename of a
    /// symbol.
    ///
    /// [`textDocument/rename`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#textDocument_rename
    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let _ = params;
        error!("Got a textDocument/rename request, but it is not implemented");
        Err(Error::method_not_found())
    }

    /// The [`textDocument/prepareRename`] request is sent from the client to the server to setup
    /// and test the validity of a rename operation at a given location.
    ///
    /// [`textDocument/prepareRename`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#textDocument_prepareRename
    ///
    /// # Compatibility
    ///
    /// This request was introduced in specification version 3.12.0.
    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let _ = params;
        error!("Got a textDocument/prepareRename request, but it is not implemented");
        Err(Error::method_not_found())
    }

    /// The [`textDocument/foldingRange`] request is sent from the client to the server to return
    /// all folding ranges found in a given text document.
    ///
    /// [`textDocument/foldingRange`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#textDocument_foldingRange
    ///
    /// # Compatibility
    ///
    /// This request was introduced in specification version 3.10.0.
    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let _ = params;
        error!("Got a textDocument/foldingRange request, but it is not implemented");
        Err(Error::method_not_found())
    }

    /// The [`textDocument/selectionRange`] request is sent from the client to the server to return
    /// suggested selection ranges at an array of given positions. A selection range is a range
    /// around the cursor position which the user might be interested in selecting.
    ///
    /// [`textDocument/selectionRange`]: https://microsoft.github.io/language-server-protocol/specifications/specification-current/#textDocument_selectionRange
    ///
    /// A selection range in the return array is for the position in the provided parameters at the
    /// same index. Therefore `params.positions[i]` must be contained in `result[i].range`.
    ///
    /// # Compatibility
    ///
    /// This request was introduced in specification version 3.15.0.
    async fn selection_range(
        &self,
        params: SelectionRangeParams,
    ) -> Result<Option<Vec<SelectionRange>>> {
        let _ = params;
        error!("Got a textDocument/selectionRange request, but it is not implemented");
        Err(Error::method_not_found())
    }
}

#[async_trait]
impl<S: ?Sized + LanguageServer> LanguageServer for Box<S> {
    async fn initialize(
        &self,
        client: &Client,
        params: InitializeParams,
    ) -> Result<InitializeResult> {
        (**self).initialize(client, params).await
    }

    async fn initialized(&self, client: &Client, params: InitializedParams) {
        (**self).initialized(client, params).await;
    }

    async fn shutdown(&self) -> Result<()> {
        (**self).shutdown().await
    }

    async fn did_change_workspace_folders(
        &self,
        client: &Client,
        params: DidChangeWorkspaceFoldersParams,
    ) {
        (**self).did_change_workspace_folders(client, params).await;
    }

    async fn did_change_configuration(
        &self,
        client: &Client,
        params: DidChangeConfigurationParams,
    ) {
        (**self).did_change_configuration(client, params).await;
    }

    async fn did_change_watched_files(&self, client: &Client, params: DidChangeWatchedFilesParams) {
        (**self).did_change_watched_files(client, params).await;
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        (**self).symbol(params).await
    }

    async fn execute_command(
        &self,
        client: &Client,
        params: ExecuteCommandParams,
    ) -> Result<Option<Value>> {
        (**self).execute_command(client, params).await
    }

    async fn did_open(&self, client: &Client, params: DidOpenTextDocumentParams) {
        (**self).did_open(client, params).await;
    }

    async fn did_change(&self, client: &Client, params: DidChangeTextDocumentParams) {
        (**self).did_change(client, params).await;
    }

    async fn will_save(&self, client: &Client, params: WillSaveTextDocumentParams) {
        (**self).will_save(client, params).await;
    }

    async fn will_save_wait_until(
        &self,
        params: WillSaveTextDocumentParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        (**self).will_save_wait_until(params).await
    }

    async fn did_save(&self, client: &Client, params: DidSaveTextDocumentParams) {
        (**self).did_save(client, params).await;
    }

    async fn did_close(&self, client: &Client, params: DidCloseTextDocumentParams) {
        (**self).did_close(client, params).await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        (**self).completion(params).await
    }

    async fn completion_resolve(&self, params: CompletionItem) -> Result<CompletionItem> {
        (**self).completion_resolve(params).await
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        (**self).hover(params).await
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        (**self).signature_help(params).await
    }

    async fn goto_declaration(
        &self,
        params: GotoDeclarationParams,
    ) -> Result<Option<GotoDeclarationResponse>> {
        (**self).goto_declaration(params).await
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        (**self).goto_definition(params).await
    }

    async fn goto_type_definition(
        &self,
        params: GotoTypeDefinitionParams,
    ) -> Result<Option<GotoTypeDefinitionResponse>> {
        (**self).goto_type_definition(params).await
    }

    async fn goto_implementation(
        &self,
        params: GotoImplementationParams,
    ) -> Result<Option<GotoImplementationResponse>> {
        (**self).goto_implementation(params).await
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        (**self).references(params).await
    }

    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        (**self).document_highlight(params).await
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        (**self).document_symbol(params).await
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

    async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        (**self).document_link(params).await
    }

    async fn document_link_resolve(&self, params: DocumentLink) -> Result<DocumentLink> {
        (**self).document_link_resolve(params).await
    }

    async fn document_color(&self, params: DocumentColorParams) -> Result<Vec<ColorInformation>> {
        (**self).document_color(params).await
    }

    async fn color_presentation(
        &self,
        params: ColorPresentationParams,
    ) -> Result<Vec<ColorPresentation>> {
        (**self).color_presentation(params).await
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        (**self).formatting(params).await
    }

    async fn range_formatting(
        &self,
        params: DocumentRangeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        (**self).range_formatting(params).await
    }

    async fn on_type_formatting(
        &self,
        params: DocumentOnTypeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        (**self).on_type_formatting(params).await
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        (**self).rename(params).await
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        (**self).prepare_rename(params).await
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        (**self).folding_range(params).await
    }

    async fn selection_range(
        &self,
        params: SelectionRangeParams,
    ) -> Result<Option<Vec<SelectionRange>>> {
        (**self).selection_range(params).await
    }
}

fn _assert_object_safe() {
    fn assert_impl<T: LanguageServer>() {}
    assert_impl::<Box<dyn LanguageServer>>();
}
