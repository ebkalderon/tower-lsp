use futures::future;
use jsonrpc_core::{BoxFuture, Result};
use serde_json::Value;
use tower_lsp::lsp_types::request::GotoDefinitionResponse;
use tower_lsp::lsp_types::*;
use tower_lsp::{LanguageServer, LspService, Printer, Server};

#[derive(Debug, Default)]
struct Backend;

impl LanguageServer for Backend {
    type ShutdownFuture = BoxFuture<()>;
    type SymbolFuture = BoxFuture<Option<Vec<SymbolInformation>>>;
    type ExecuteFuture = BoxFuture<Option<Value>>;
    type CompletionFuture = BoxFuture<Option<CompletionResponse>>;
    type HoverFuture = BoxFuture<Option<Hover>>;
    type DeclarationFuture = BoxFuture<Option<GotoDefinitionResponse>>;
    type DefinitionFuture = BoxFuture<Option<GotoDefinitionResponse>>;
    type TypeDefinitionFuture = BoxFuture<Option<GotoDefinitionResponse>>;
    type HighlightFuture = BoxFuture<Option<Vec<DocumentHighlight>>>;

    fn initialize(&self, _: &Printer, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: None,
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::Incremental,
                )),
                hover_provider: Some(true),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec![".".to_string()]),
                    work_done_progress_options: Default::default(),
                }),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: None,
                    retrigger_characters: None,
                    work_done_progress_options: Default::default(),
                }),
                document_highlight_provider: Some(true),
                workspace_symbol_provider: Some(true),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec!["dummy.do_something".to_string()],
                    work_done_progress_options: Default::default(),
                }),
                workspace: Some(WorkspaceCapability {
                    workspace_folders: Some(WorkspaceFolderCapability {
                        supported: Some(true),
                        change_notifications: Some(
                            WorkspaceFolderCapabilityChangeNotifications::Bool(true),
                        ),
                    }),
                }),
                ..ServerCapabilities::default()
            },
        })
    }

    fn initialized(&self, printer: &Printer, _: InitializedParams) {
        printer.log_message(MessageType::Info, "server initialized!");
    }

    fn shutdown(&self) -> Self::ShutdownFuture {
        Box::new(future::ok(()))
    }

    fn symbol(&self, _: WorkspaceSymbolParams) -> Self::SymbolFuture {
        Box::new(future::ok(None))
    }

    fn did_change_workspace_folders(&self, printer: &Printer, _: DidChangeWorkspaceFoldersParams) {
        printer.log_message(MessageType::Info, "workspace folders changed!");
    }

    fn did_change_configuration(&self, printer: &Printer, _: DidChangeConfigurationParams) {
        printer.log_message(MessageType::Info, "configuration changed!");
    }

    fn did_change_watched_files(&self, printer: &Printer, _: DidChangeWatchedFilesParams) {
        printer.log_message(MessageType::Info, "watched files have changed!");
    }

    fn execute_command(&self, printer: &Printer, _: ExecuteCommandParams) -> Self::ExecuteFuture {
        printer.log_message(MessageType::Info, "command executed!");
        printer.apply_edit(WorkspaceEdit::default());
        Box::new(future::ok(None))
    }

    fn did_open(&self, printer: &Printer, _: DidOpenTextDocumentParams) {
        printer.log_message(MessageType::Info, "file opened!");
    }

    fn did_change(&self, printer: &Printer, _: DidChangeTextDocumentParams) {
        printer.log_message(MessageType::Info, "file changed!");
    }

    fn did_save(&self, printer: &Printer, _: DidSaveTextDocumentParams) {
        printer.log_message(MessageType::Info, "file saved!");
    }

    fn did_close(&self, printer: &Printer, _: DidCloseTextDocumentParams) {
        printer.log_message(MessageType::Info, "file closed!");
    }

    fn completion(&self, _: CompletionParams) -> Self::CompletionFuture {
        Box::new(future::ok(None))
    }

    fn hover(&self, _: TextDocumentPositionParams) -> Self::HoverFuture {
        Box::new(future::ok(None))
    }

    fn goto_declaration(&self, _: TextDocumentPositionParams) -> Self::DeclarationFuture {
        Box::new(future::ok(None))
    }

    fn goto_definition(&self, _: TextDocumentPositionParams) -> Self::DefinitionFuture {
        Box::new(future::ok(None))
    }

    fn goto_type_definition(&self, _: TextDocumentPositionParams) -> Self::TypeDefinitionFuture {
        Box::new(future::ok(None))
    }

    fn document_highlight(&self, _: TextDocumentPositionParams) -> Self::HighlightFuture {
        Box::new(future::ok(None))
    }
}

fn main() {
    env_logger::init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, messages) = LspService::new(Backend::default());
    let handle = service.close_handle();
    let server = Server::new(stdin, stdout)
        .interleave(messages)
        .serve(service);

    tokio::run(handle.run_until_exit(server));
}
