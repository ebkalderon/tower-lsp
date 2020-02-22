# tower-lsp

[![Build Status][build-badge]][build-url]
[![Crates.io][crates-badge]][crates-url]
[![Documentation][docs-badge]][docs-url]

[build-badge]: https://github.com/ebkalderon/tower-lsp/workflows/rust/badge.svg
[build-url]: https://github.com/ebkalderon/tower-lsp/actions
[crates-badge]: https://img.shields.io/crates/v/tower-lsp.svg
[crates-url]: https://crates.io/crates/tower-lsp
[docs-badge]: https://docs.rs/tower-lsp/badge.svg
[docs-url]: https://docs.rs/tower-lsp

[Language Server Protocol] implementation for Rust based on [Tower].

[Language Server Protocol]: https://microsoft.github.io/language-server-protocol
[Tower]: https://github.com/tower-rs/tower

Tower is a simple and composable framework for implementing asynchronous
services in Rust. Central to Tower is the [`Service`] trait, which provides the
necessary abstractions for defining request/response clients and servers.
Examples of protocols implemented using the `Service` trait include
[`tower-web`] and [`tower-grpc`].

[`Service`]: https://docs.rs/tower-service/
[`tower-web`]: https://docs.rs/tower-web/
[`tower-grpc`]: https://docs.rs/tower-grpc/

This library (`tower-lsp`) provides a simple implementation of the Language
Server Protocol (LSP) that makes it easy to write your own language server. It
consists of three parts:

* The `LanguageServer` trait which defines the behavior of your language server.
* The asynchronous `LspService` delegate which wraps your language server
  implementation and defines the behavior of the protocol.
* A `Server` which spawns the `LspService` and processes requests and responses
  over stdin and stdout.

## Example

```rust
use jsonrpc_core::Result;
use serde_json::Value;
use tower_lsp::lsp_types::request::*;
use tower_lsp::lsp_types::*;
use tower_lsp::{LanguageServer, LspService, Printer, Server};

#[derive(Debug, Default)]
struct Backend;

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    fn initialize(&self, _: &Printer, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult::default())
    }

    fn initialized(&self, printer: &Printer, _: InitializedParams) {
        printer.log_message(MessageType::Info, "server initialized!");
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn symbol(&self, _: WorkspaceSymbolParams) -> Result<Option<Vec<SymbolInformation>>> {
        Ok(None)
    }

    async fn execute_command(&self, _: &Printer, _: ExecuteCommandParams) -> Result<Option<Value>> {
        Ok(None)
    }

    async fn completion(&self, _: CompletionParams) -> Result<Option<CompletionResponse>> {
        Ok(None)
    }

    async fn hover(&self, _: TextDocumentPositionParams) -> Result<Option<Hover>> {
        Ok(None)
    }

    async fn signature_help(&self, _: TextDocumentPositionParams) -> Result<Option<SignatureHelp>> {
        Ok(None)
    }

    async fn goto_declaration(&self, _: TextDocumentPositionParams) -> Result<Option<GotoDefinitionResponse>> {
        Ok(None)
    }

    async fn goto_definition(&self, _: TextDocumentPositionParams) -> Result<Option<GotoDefinitionResponse>> {
        Ok(None)
    }

    async fn goto_type_definition(&self, _: TextDocumentPositionParams) -> Result<Option<GotoDefinitionResponse>> {
        Ok(None)
    }

    async fn goto_implementation(&self, _: TextDocumentPositionParams) -> Result<Option<GotoImplementationResponse>> {
        Ok(None)
    }

    async fn document_highlight(&self, _: TextDocumentPositionParams) -> Result<Option<Vec<DocumentHighlight>>> {
        Ok(None)
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, messages) = LspService::new(Backend::default());
    let handle = service.close_handle();
    let server = Server::new(stdin, stdout)
        .interleave(messages)
        .serve(service);

    handle.run_until_exit(server).await;
}
```

## License

`tower-lsp` is free and open source software distributed under the terms of
both the [MIT](LICENSE-MIT) and the [Apache 2.0](LICENSE-APACHE) licenses.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
