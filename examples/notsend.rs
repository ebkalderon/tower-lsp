use serde_json::Value;
use std::borrow::Cow;
use std::rc::Rc;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

/// ☠️ Let it be known
///
/// This example involves a very specific situation where you have a `!Send` or `!Sync` variable
/// that cannot be held across an await point, that you do want to hold across an await point.
/// If you do not have this limitation you should not follow this example.
/// It will cause you to serialize data between the `LanguageServer` trait and your backend.
///
/// But if you do encounter such a scenario, this example shows that you can still use tower-lsp.
/// However there are some contortions involved.
///
/// ☠️

/// Here is our `!Send + !Sync` structure, it is so because it embed's an RC.
/// Normally this is would all go in `Backend`.
#[derive(Default)]
struct FooState {
    // Lets pretend this is some useful state we derive from DidOpen.
    // then we need again in e.g. DidChange so we would want to hold it across an await point.
    // This could be something like an AST or an interpreter,
    // Here we'll use the length of the text during DidOpen for simplicity.
    text_len: Option<Rc<usize>>,
}

/// These are for notifications coming from our thread back to the editor.
#[derive(Debug)]
enum FromFoo {
    Info(std::borrow::Cow<'static, str>),
}

/// Messages coming from the editor via Backend to our thread.
#[derive(Debug)]
enum ToFoo {
    DidOpen {
        params: DidOpenTextDocumentParams,
    },
    DidChange {
        params: DidChangeTextDocumentParams,
    },
    ExecuteCmd {
        params: ExecuteCommandParams,
        response_channel: oneshot::Sender<FromFoo>,
    },
}

struct FooThread {
    output: mpsc::UnboundedSender<FromFoo>,
    input: mpsc::UnboundedReceiver<ToFoo>,
    shutdown: tokio::sync::broadcast::Receiver<()>,
}

impl FooThread {
    fn init(mut self: FooThread) -> impl FnOnce() {
        move || {
            let mut state: FooState = Default::default();

            while let Err(tokio::sync::broadcast::error::TryRecvError::Empty) =
                self.shutdown.try_recv()
            {
                if let Ok(input) = self.input.try_recv() {
                    use ToFoo::*;
                    match input {
                        DidOpen { params } => {
                            self.output
                                .send(FromFoo::Info(Cow::from(format!(
                                    "did open {}",
                                    params.text_document.uri
                                ))))
                                .unwrap();
                            state.text_len = Some(Rc::new(params.text_document.text.len()));
                        }

                        DidChange { params } => {
                            let _ = params;
                            self.output
                                .send(FromFoo::Info(Cow::from(format!(
                                    "did change: the text_len on open was {:?} ",
                                    state.text_len
                                ))))
                                .unwrap();
                        }
                        ExecuteCmd {
                            params,
                            response_channel,
                        } => {
                            response_channel
                                .send(FromFoo::Info(Cow::from(format!(
                                    "executed command: {} {:?}",
                                    params.command, state.text_len
                                ))))
                                .unwrap();
                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
struct BackendState {
    shutdown: broadcast::Sender<()>,
    to_foo: Option<mpsc::UnboundedSender<ToFoo>>,
    // uncomment for:
    // Rc<usize> cannot be sent between threads safely
    // bar: Rc<usize>,
}

#[derive(Debug)]
struct Backend {
    client: Client,
    state: Mutex<BackendState>,
}

async fn process_foo_notifications<'a>(
    client: tower_lsp::Client,
    mut from_foo: mpsc::UnboundedReceiver<FromFoo>,
    mut shutdown: broadcast::Receiver<()>,
) {
    while let Err(broadcast::error::TryRecvError::Empty) = shutdown.try_recv() {
        if let Some(notif) = from_foo.recv().await {
            match notif {
                FromFoo::Info(msg) => {
                    client.log_message(MessageType::INFO, msg).await;
                }
            }
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        let mut state = self.state.lock().await;
        let (to_foo, foo_input) = mpsc::unbounded_channel();
        let (foo_output, from_foo) = mpsc::unbounded_channel();

        state.to_foo = Some(to_foo);
        let _ = tokio::task::spawn(process_foo_notifications(
            self.client.clone(),
            from_foo,
            state.shutdown.subscribe(),
        ));
        std::thread::spawn(FooThread::init(FooThread {
            output: foo_output,
            input: foo_input,
            shutdown: state.shutdown.subscribe(),
        }));

        Ok(InitializeResult {
            server_info: None,
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec!["dummy.do_something".to_string()],
                    work_done_progress_options: Default::default(),
                }),
                ..ServerCapabilities::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        let state = self.state.lock().await;
        state.shutdown.send(()).unwrap();
        Ok(())
    }

    async fn execute_command(&self, params: ExecuteCommandParams) -> Result<Option<Value>> {
        let state = self.state.lock().await;
        let (response_channel, from_foo) = oneshot::channel();
        // Here we aren't dealing with a notification response, but we actually need a result to return
        // So rather than responses coming through process_foo_notifications, we have to send it
        // a channel and wait for the response.
        if params.command == "dummy.do_something" {
            state
                .to_foo
                .as_ref()
                .unwrap()
                .send(ToFoo::ExecuteCmd {
                    params,
                    response_channel,
                })
                .unwrap();
            if let Ok(FromFoo::Info(msg)) = from_foo.await {
                return Ok(Some(msg.into()));
            }
        }

        Ok(None)
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let state = self.state.lock().await;
        // We can do this:
        let _text_len = Rc::new(params.text_document.text.len());
        state
            .to_foo
            .as_ref()
            .unwrap()
            .send(ToFoo::DidOpen { params })
            .unwrap();
        // uncomment for:
        // future cannot be sent safely between threads
        // self.client.log_message(MessageType::INFO, "However we can't hold _text_len across this await point").await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let state = self.state.lock().await;
        state
            .to_foo
            .as_ref()
            .unwrap()
            .send(ToFoo::DidChange { params })
            .unwrap();
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());
    let (shutdown, _) = broadcast::channel(1);
    let state = Mutex::new(BackendState {
        to_foo: None,
        shutdown,
    });
    let (service, socket) = LspService::new(|client| Backend { client, state });
    Server::new(stdin, stdout, socket).serve(service).await;
}
