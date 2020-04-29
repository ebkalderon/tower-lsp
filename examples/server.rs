use serde_json::Value;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

#[derive(Debug, Default)]
struct Backend;

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: &Client, _: InitializeParams) -> Result<InitializeResult> {
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

    async fn initialized(&self, client: &Client, _: InitializedParams) {
        client.log_message(MessageType::Info, "server initialized!");
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_change_workspace_folders(
        &self,
        client: &Client,
        _: DidChangeWorkspaceFoldersParams,
    ) {
        client.log_message(MessageType::Info, "workspace folders changed!");
    }

    async fn did_change_configuration(&self, client: &Client, _: DidChangeConfigurationParams) {
        client.log_message(MessageType::Info, "configuration changed!");
    }

    async fn did_change_watched_files(&self, client: &Client, _: DidChangeWatchedFilesParams) {
        client.log_message(MessageType::Info, "watched files have changed!");
    }

    async fn execute_command(
        &self,
        client: &Client,
        _: ExecuteCommandParams,
    ) -> Result<Option<Value>> {
        client.log_message(MessageType::Info, "command executed!");

        match client.apply_edit(WorkspaceEdit::default()).await {
            Ok(res) if res.applied => client.log_message(MessageType::Info, "edit applied"),
            Ok(_) => client.log_message(MessageType::Info, "edit not applied"),
            Err(err) => client.log_message(MessageType::Error, err),
        }

        Ok(None)
    }

    async fn did_open(&self, client: &Client, _: DidOpenTextDocumentParams) {
        client.log_message(MessageType::Info, "file opened!");
    }

    async fn did_change(&self, client: &Client, _: DidChangeTextDocumentParams) {
        client.log_message(MessageType::Info, "file changed!");
    }

    async fn did_save(&self, client: &Client, _: DidSaveTextDocumentParams) {
        client.log_message(MessageType::Info, "file saved!");
    }

    async fn did_close(&self, client: &Client, _: DidCloseTextDocumentParams) {
        client.log_message(MessageType::Info, "file closed!");
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
    Server::new(stdin, stdout)
        .interleave(messages)
        .serve(service)
        .await;
}
