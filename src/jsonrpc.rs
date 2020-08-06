//! A subset of JSON-RPC types used by the Language Server Protocol.

pub use jsonrpc_core::params::Params;
pub use jsonrpc_core::request::{MethodCall, Notification};
pub use jsonrpc_core::response::{Failure, Output, Success};

pub use self::error::{Error, ErrorCode};

use std::fmt::{self, Debug, Display, Formatter};

use serde::de::{self, Deserializer};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod error;

/// A specialized [`Result`] error type for JSON-RPC handlers.
///
/// [`Result`]: enum@std::result::Result
pub type Result<T> = std::result::Result<T, Error>;

/// A unique ID used to correlate requests and responses together.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Id {
    /// Numeric ID.
    Number(u64),
    /// String ID.
    String(String),
}

impl Display for Id {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Id::Number(id) => Display::fmt(id, f),
            Id::String(id) => Debug::fmt(id, f),
        }
    }
}

/// A successful or failed JSON-RPC response.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Response {
    jsonrpc: Version,
    #[serde(flatten)]
    kind: ResponseKind,
}

impl Response {
    /// Creates a new successful response from a request ID and `Error` object.
    #[inline]
    pub const fn ok(id: Id, result: Value) -> Self {
        Response {
            jsonrpc: Version,
            kind: ResponseKind::Ok { result, id },
        }
    }

    /// Creates a new error response from a request ID and `Error` object.
    #[inline]
    pub const fn error(id: Option<Id>, error: Error) -> Self {
        Response {
            jsonrpc: Version,
            kind: ResponseKind::Err { error, id },
        }
    }

    /// Creates a new response from a request ID and either an `Ok(Value)` or `Err(Error)` body.
    #[inline]
    pub fn from_parts(id: Id, body: Result<Value>) -> Self {
        match body {
            Ok(result) => Response::ok(id, result),
            Err(error) => Response::error(Some(id), error),
        }
    }

    /// Splits the response into a request ID paired with either an `Ok(Value)` or `Err(Error)` to
    /// signify whether the response is a success or failure.
    #[inline]
    pub fn into_parts(self) -> (Option<Id>, Result<Value>) {
        match self.kind {
            ResponseKind::Ok { id, result } => (Some(id), Ok(result)),
            ResponseKind::Err { id, error } => (id, Err(error)),
        }
    }

    /// Returns the corresponding request ID, if any.
    #[inline]
    pub fn id(&self) -> Option<&Id> {
        match self.kind {
            ResponseKind::Ok { ref id, .. } => Some(id),
            ResponseKind::Err { ref id, .. } => id.as_ref(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(untagged)]
enum ResponseKind {
    Ok { result: Value, id: Id },
    Err { error: Error, id: Option<Id> },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Version;

impl<'a> Deserialize<'a> for Version {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'a>,
    {
        match Deserialize::deserialize(deserializer)? {
            "2.0" => Ok(Version),
            _ => Err(de::Error::custom("expected JSON-RPC version \"2.0\"")),
        }
    }
}

impl Serialize for Version {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        "2.0".serialize(serializer)
    }
}
