use serde::{Deserialize, Serialize};
use serde_json::Value;
use tower_lsp::jsonrpc::{Error, Result};
use tower_lsp::lsp_types::notification::Notification;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

#[derive(Debug, Deserialize, Serialize)]
struct CustomNotificationParams {
    title: String,
    message: String,
}

impl CustomNotificationParams {
    fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        CustomNotificationParams {
            title: title.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug)]
enum CustomNotification {}

impl Notification for CustomNotification {
    type Params = CustomNotificationParams;
    const METHOD: &'static str = "custom/notification";
}

#[derive(Debug, Default)]
struct Backend;

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: &Client, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: None,
            capabilities: ServerCapabilities {
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec!["custom.notification".to_string()],
                    work_done_progress_options: Default::default(),
                }),
                ..ServerCapabilities::default()
            },
        })
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn execute_command(
        &self,
        client: &Client,
        params: ExecuteCommandParams,
    ) -> Result<Option<Value>> {
        if params.command == "custom.notification" {
            client.send_custom_notification::<CustomNotification>(CustomNotificationParams::new(
                "Hello", "Message",
            ));
            client.log_message(
                MessageType::Info,
                format!("Command executed with params: {:?}", params),
            );
            Ok(None)
        } else {
            Err(Error::invalid_request())
        }
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
