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
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

#[derive(Debug, Default)]
struct Backend;

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: &Client, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult::default())
    }

    async fn initialized(&self, client: &Client, _: InitializedParams) {
        client.log_message(MessageType::Info, "server initialized!");
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, messages) = LspService::new(Backend::default());
    Server::new(stdin, stdout)
        .interleave(messages)
        .serve(service)
        .await;
}
```

## License

`tower-lsp` is free and open source software distributed under the terms of
both the [MIT](LICENSE-MIT) and the [Apache 2.0](LICENSE-APACHE) licenses.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
