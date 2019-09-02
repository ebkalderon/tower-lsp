use futures::future;
use jsonrpc_core::{BoxFuture, Result};
use tower_lsp::lsp_types::*;
use tower_lsp::{LanguageServer, LspService, Printer, Server};

#[derive(Debug, Default)]
struct Backend;

impl LanguageServer for Backend {
    type ShutdownFuture = BoxFuture<()>;
    type HighlightFuture = BoxFuture<Option<Vec<DocumentHighlight>>>;
    type HoverFuture = BoxFuture<Option<Hover>>;

    fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult::default())
    }

    fn initialized(&self, printer: &Printer, _: InitializedParams) {
        printer.log_message(MessageType::Info, "server initialized!");
    }

    fn shutdown(&self) -> Self::ShutdownFuture {
        Box::new(future::ok(()))
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

    fn hover(&self, _: TextDocumentPositionParams) -> Self::HoverFuture {
        Box::new(future::ok(None))
    }

    fn highlight(&self, _: TextDocumentPositionParams) -> Self::HighlightFuture {
        Box::new(future::ok(None))
    }
}

fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, messages) = LspService::new(Backend::default());
    let handle = service.close_handle();
    let server = Server::new(stdin, stdout)
        .interleave(messages)
        .serve(service);

    tokio::run(handle.run_until_exit(server));
}
