#![forbid(unsafe_code)]

/// A re-export of [`async-trait`](https://docs.rs/async-trait) for convenience.
pub use async_trait::async_trait;

pub mod jsonrpc;

mod codec;

#[async_trait]
pub trait LanguageServer: 'static {
    /// The [`initialize`] request is the first request sent from the client to the server.
    ///
    /// [`initialize`]: https://microsoft.github.io/language-server-protocol/specification#initialize
    ///
    /// This method is guaranteed to only execute once. If the client sends this request to the
    /// server again, the server will respond with JSON-RPC error code `-32600` (invalid request).
    async fn initialize(&self, params: usize) -> Result<usize, ()>;

    /// The [`initialized`] notification is sent from the client to the server after the client
    /// received the result of the initialize request but before the client sends anything else.
    ///
    /// [`initialized`]: https://microsoft.github.io/language-server-protocol/specification#initialized
    ///
    /// The server can use the `initialized` notification, for example, to dynamically register
    /// capabilities with the client.
    async fn initialized(&mut self, params: u32) {
        let _ = params;
    }
}
