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
    /// Creates a new successful response from a response ID and `Error` object.
    pub const fn from_ok(id: Id, result: Value) -> Self {
        Response {
            jsonrpc: Version,
            kind: Kind::Ok { result },
            id,
        }
    }

    /// Creates a new error response from a response ID and `Error` object.
    pub const fn from_error(id: Id, error: Error) -> Self {
        Response {
            jsonrpc: Version,
            kind: Kind::Err { error },
            id,
        }
    }

    /// Creates a new response from a response ID and either an `Ok(Value)` or `Err(Error)` body.
    pub fn from_parts(id: Id, body: Result<Value>) -> Self {
        match body {
            Ok(result) => Response::from_ok(id, result),
            Err(error) => Response::from_error(id, error),
        }
    }

    /// Splits the response into a response ID paired with either an `Ok(Value)` or `Err(Error)` to
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

    /// Returns the corresponding response ID, if known.
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn deserializes_ok_response() {
        let response = Response::from_str(r#"{"jsonrpc":"2.0","result":123,"id":1}"#);
        let expected = Response::from_ok(Id::Number(1), json!(123u32));

        assert_eq!(response.unwrap(), expected);
    }

    #[test]
    fn deserializes_error_response() {
        let response = Response::from_str(
            r#"{"jsonrpc":"2.0","error":{"code":-32700,"message":"Parse error"},"id":1}"#,
        );
        let expected = Response::from_error(Id::Number(1), Error::parse_error());

        assert_eq!(response.unwrap(), expected);
    }

    #[test]
    fn deserializes_error_response_with_null_id() {
        let response = Response::from_str(
            r#"{"jsonrpc":"2.0","error":{"code":-32700,"message":"Parse error"},"id":null}"#,
        );
        let expected = Response::from_error(Id::Null, Error::parse_error());

        assert_eq!(response.unwrap(), expected);
    }

    #[test]
    fn deserializes_error_response_with_data() {
        let response = Response::from_str(
            r#"{"jsonrpc":"2.0","error":{"code":-32700,"message":"Parse error","data":123},"id":1}"#,
        );
        let expected = Response::from_error(
            Id::Number(1),
            Error {
                data: Some(json!(123u32)),
                ..Error::parse_error()
            },
        );

        assert_eq!(response.unwrap(), expected);
    }

    #[test]
    fn rejects_invalid_jsonrpc_version() {
        Response::from_str(r#"{"jsonrpc":"1.0","result":123,"id":1}"#).unwrap_err();
        Response::from_str(
            r#"{"jsonrpc":null,"error":{"code":-32700,"message":"Parse error"},"id":1}"#,
        )
        .unwrap_err();
    }

    #[test]
    fn rejects_invalid_error() {
        Response::from_str(r#"{"jsonrpc":"2.0","error":"invalid","id":1}"#).unwrap_err();
    }

    #[test]
    fn rejects_missing_result_or_error() {
        Response::from_str(r#"{"jsonrpc":"2.0","id":1}"#).unwrap_err();
    }

    #[test]
    fn rejects_invalid_ids() {
        // FIXME: This probably shouldn't be allowed. Will handle in a later `Id` refactor.
        // Response::from_str(r#"{"jsonrpc":"2.0","result":123,"id":null}"#).unwrap_err();
        Response::from_str(r#"{"jsonrpc":"2.0","result":123,"id":[]}"#).unwrap_err();
        Response::from_str(r#"{"jsonrpc":"2.0","result":123,"id":{}}"#).unwrap_err();
        Response::from_str(r#"{"jsonrpc":"2.0","result":123,"id":true}"#).unwrap_err();
        Response::from_str(
            r#"{"jsonrpc":"2.0","error":{"code":-32700,"message":"Parse error"},"id":[]}"#,
        )
        .unwrap_err();
        Response::from_str(
            r#"{"jsonrpc":"2.0","error":{"code":-32700,"message":"Parse error"},"id":{}}"#,
        )
        .unwrap_err();
        Response::from_str(
            r#"{"jsonrpc":"2.0","error":{"code":-32700,"message":"Parse error"},"id":true}"#,
        )
        .unwrap_err();
    }

    #[test]
    fn rejects_missing_id() {
        Response::from_str(r#"{"jsonrpc":"2.0","result":123}"#).unwrap_err();
        Response::from_str(r#"{"jsonrpc":"2.0","error":{"code":-32700,"message":"Parse error"}}"#)
            .unwrap_err();
    }

    #[test]
    fn rejects_invalid_syntax() {
        Response::from_str(r#"fn main() { println!("This isn't JSON at all!"); }"#).unwrap_err();
    }
}
