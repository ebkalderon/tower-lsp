//! Re-exports of common [`jsonrpc-core`](https://docs.rs/jsonrpc-core) types for convenience.

pub use jsonrpc_core::error::{Error, ErrorCode};
pub use jsonrpc_core::id::Id;
pub use jsonrpc_core::params::Params;
pub use jsonrpc_core::request::{MethodCall, Notification};
pub use jsonrpc_core::response::{Failure, Output, Success};
pub use jsonrpc_core::version::Version;
pub use jsonrpc_core::Result;
