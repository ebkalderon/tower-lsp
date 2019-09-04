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

    fn hover(&self, _: TextDocumentPositionParams) -> Self::HoverFuture {
        Box::new(future::ok(None))
    }

    fn document_highlight(&self, _: TextDocumentPositionParams) -> Self::HighlightFuture {
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
```

## License

`tower-lsp` is free and open source software distributed under the terms of
both the [MIT](LICENSE-MIT) and the [Apache 2.0](LICENSE-APACHE) licenses.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
