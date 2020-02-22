use jsonrpc_core::Result;
use serde_json::Value;
use tower_lsp::lsp_types::*;
use tower_lsp::{LanguageServer, LspService, Printer, Server};

#[derive(Debug, Default)]
struct Backend;

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
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

    async fn shutdown(&self) -> Result<()> {
        Ok(())
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

    async fn execute_command(
        &self,
        printer: &Printer,
        _: ExecuteCommandParams,
    ) -> Result<Option<Value>> {
        printer.log_message(MessageType::Info, "command executed!");
        printer.apply_edit(WorkspaceEdit::default());
        Ok(None)
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

    async fn completion(&self, _: CompletionParams) -> Result<Option<CompletionResponse>> {
        Ok(Some(CompletionResponse::Array(vec![
            CompletionItem::new_simple("Hello".to_string(), "Some detail".to_string()),
            CompletionItem::new_simple("Bye".to_string(), "More detail".to_string()),
        ])))
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, messages) = LspService::new(Backend::default());
    let handle = service.close_handle();
    let server = Server::new(stdin, stdout)
        .interleave(messages)
        .serve(service);

    handle.run_until_exit(server).await;
}
