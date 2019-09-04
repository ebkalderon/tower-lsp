//! Messages understood by the Language Server Protocol.

use std::fmt::{Display, Formatter, Result as FmtResult};
use std::str::FromStr;

use jsonrpc_core::types::request::{MethodCall, Notification};
use jsonrpc_core::types::response::Output;
use serde::{Deserialize, Serialize};
use serde_json::Error;

/// An incoming JSON-RPC message.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Incoming {
    /// Request sent from the client to the server.
    ///
    /// This incoming message will produce a response.
    Request(MethodCall),
    /// Notification sent from the client to the server.
    ///
    /// This incoming message will not produce a response.
    Notification(Notification),
    /// Response sent from the client to the server.
    ///
    /// This incoming message will not produce a response.
    Response(Output),
    /// An unrecognized incoming message.
    ///
    /// This incoming message will produce a response.
    #[serde(skip)]
    Invalid(String),
}

impl Display for Incoming {
    fn fmt(&self, fmt: &mut Formatter) -> FmtResult {
        match *self {
            Incoming::Request(ref req) => fmt.write_str(&serde_json::to_string(req).unwrap()),
            Incoming::Notification(ref n) => fmt.write_str(&serde_json::to_string(n).unwrap()),
            Incoming::Response(ref res) => fmt.write_str(&serde_json::to_string(res).unwrap()),
            Incoming::Invalid(ref s) => fmt.write_str(s),
        }
    }
}

impl From<String> for Incoming {
    fn from(s: String) -> Self {
        Incoming::from_str(&s).unwrap_or_else(|_| Incoming::Invalid(s))
    }
}

impl FromStr for Incoming {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}
