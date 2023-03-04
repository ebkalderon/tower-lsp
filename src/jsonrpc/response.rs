use std::fmt::{self, Debug, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Error, Id, Result, Version};

#[derive(Clone, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
enum Kind {
    Ok { result: Value },
    Err { error: Error },
}

/// A successful or failed JSON-RPC response.
#[derive(Clone, PartialEq, Deserialize, Serialize)]
pub struct Response {
    jsonrpc: Version,
    #[serde(flatten)]
    kind: Kind,
    id: Id,
}

impl Response {
    /// Creates a new successful response from a request ID and `Error` object.
    pub const fn from_ok(id: Id, result: Value) -> Self {
        Response {
            jsonrpc: Version,
            kind: Kind::Ok { result },
            id,
        }
    }

    /// Creates a new error response from a request ID and `Error` object.
    pub const fn from_error(id: Id, error: Error) -> Self {
        Response {
            jsonrpc: Version,
            kind: Kind::Err { error },
            id,
        }
    }

    /// Creates a new response from a request ID and either an `Ok(Value)` or `Err(Error)` body.
    pub fn from_parts(id: Id, body: Result<Value>) -> Self {
        match body {
            Ok(result) => Response::from_ok(id, result),
            Err(error) => Response::from_error(id, error),
        }
    }

    /// Splits the response into a request ID paired with either an `Ok(Value)` or `Err(Error)` to
    /// signify whether the response is a success or failure.
    pub fn into_parts(self) -> (Id, Result<Value>) {
        match self.kind {
            Kind::Ok { result } => (self.id, Ok(result)),
            Kind::Err { error } => (self.id, Err(error)),
        }
    }

    /// Returns `true` if the response indicates success.
    pub const fn is_ok(&self) -> bool {
        matches!(self.kind, Kind::Ok { .. })
    }

    /// Returns `true` if the response indicates failure.
    pub const fn is_error(&self) -> bool {
        !self.is_ok()
    }

    /// Returns the `result` value, if it exists.
    ///
    /// This member only exists if the response indicates success.
    pub const fn result(&self) -> Option<&Value> {
        match &self.kind {
            Kind::Ok { result } => Some(result),
            _ => None,
        }
    }

    /// Returns the `error` value, if it exists.
    ///
    /// This member only exists if the response indicates failure.
    pub const fn error(&self) -> Option<&Error> {
        match &self.kind {
            Kind::Err { error } => Some(error),
            _ => None,
        }
    }

    /// Returns the corresponding request ID, if known.
    pub const fn id(&self) -> &Id {
        &self.id
    }
}

impl Debug for Response {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut d = f.debug_struct("Response");
        d.field("jsonrpc", &self.jsonrpc);

        match &self.kind {
            Kind::Ok { result } => d.field("result", result),
            Kind::Err { error } => d.field("error", error),
        };

        d.field("id", &self.id).finish()
    }
}

impl FromStr for Response {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}
