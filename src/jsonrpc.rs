//! A subset of JSON-RPC types used by the Language Server Protocol.

pub use self::error::{Error, ErrorCode, Result};
pub use self::request::{Request, RequestBuilder};
pub(crate) use self::router::Router;
pub use self::router::{FromParams, IntoResponse, Method};

use std::borrow::Cow;
use std::fmt::{self, Debug, Display, Formatter};

use lsp_types::NumberOrString;
use serde::de::{self, Deserializer};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod error;
mod request;
mod router;

/// A unique ID used to correlate requests and responses together.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Id {
    /// Numeric ID.
    Number(i64),
    /// String ID.
    String(String),
    /// Null ID.
    ///
    /// While `null` is considered a valid request ID by the JSON-RPC 2.0 specification, its use is
    /// _strongly_ discouraged because the specification also uses a `null` value to indicate an
    /// unknown ID in the [`Response`] object.
    Null,
}

impl Default for Id {
    fn default() -> Self {
        Id::Null
    }
}

impl Display for Id {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Id::Number(id) => Display::fmt(id, f),
            Id::String(id) => Debug::fmt(id, f),
            Id::Null => f.write_str("null"),
        }
    }
}

impl From<i64> for Id {
    fn from(n: i64) -> Self {
        Id::Number(n)
    }
}

impl From<&'_ str> for Id {
    fn from(s: &'_ str) -> Self {
        Id::String(s.to_string())
    }
}

impl From<String> for Id {
    fn from(s: String) -> Self {
        Id::String(s)
    }
}

impl From<NumberOrString> for Id {
    fn from(num_or_str: NumberOrString) -> Self {
        match num_or_str {
            NumberOrString::Number(num) => Id::Number(num as i64),
            NumberOrString::String(s) => Id::String(s),
        }
    }
}

/// A successful or failed JSON-RPC response.
#[derive(Clone, PartialEq, Deserialize, Serialize)]
pub struct Response {
    jsonrpc: Version,
    #[serde(flatten)]
    kind: ResponseKind,
    id: Id,
}

impl Response {
    /// Creates a new successful response from a request ID and `Error` object.
    pub const fn from_ok(id: Id, result: Value) -> Self {
        Response {
            jsonrpc: Version,
            kind: ResponseKind::Ok { result },
            id,
        }
    }

    /// Creates a new error response from a request ID and `Error` object.
    pub const fn from_error(id: Id, error: Error) -> Self {
        Response {
            jsonrpc: Version,
            kind: ResponseKind::Err { error },
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
            ResponseKind::Ok { result } => (self.id, Ok(result)),
            ResponseKind::Err { error } => (self.id, Err(error)),
        }
    }

    /// Returns `true` if the response indicates success.
    pub const fn is_ok(&self) -> bool {
        matches!(self.kind, ResponseKind::Ok { .. })
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
            ResponseKind::Ok { result } => Some(result),
            _ => None,
        }
    }

    /// Returns the `error` value, if it exists.
    ///
    /// This member only exists if the response indicates failure.
    pub const fn error(&self) -> Option<&Error> {
        match &self.kind {
            ResponseKind::Err { error } => Some(error),
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
            ResponseKind::Ok { result } => d.field("result", result),
            ResponseKind::Err { error } => d.field("error", error),
        };

        d.field("id", &self.id).finish()
    }
}

#[derive(Clone, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
enum ResponseKind {
    Ok { result: Value },
    Err { error: Error },
}

/// An incoming or outgoing JSON-RPC message.
#[derive(Deserialize, Serialize)]
#[cfg_attr(test, derive(Debug, PartialEq))]
#[serde(untagged)]
pub(crate) enum Message {
    /// A response message.
    Response(Response),
    /// A request or notification message.
    Request(Request),
}

#[derive(Clone, Debug, PartialEq)]
struct Version;

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Inner<'a>(#[serde(borrow)] Cow<'a, str>);

        let Inner(ver) = Inner::deserialize(deserializer)?;

        match ver.as_ref() {
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
        serializer.serialize_str("2.0")
    }
}

/// Error response returned for every request received before the server is initialized.
///
/// See [here](https://microsoft.github.io/language-server-protocol/specification#initialize)
/// for reference.
pub(crate) const fn not_initialized_error() -> Error {
    Error {
        code: ErrorCode::ServerError(-32002),
        message: Cow::Borrowed("Server not initialized"),
        data: None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn incoming_from_str_or_value() {
        let v = json!({"jsonrpc":"2.0","method":"initialize","params":{"capabilities":{}},"id":0});
        let from_str: Message = serde_json::from_str(&v.to_string()).unwrap();
        let from_value: Message = serde_json::from_value(v).unwrap();
        assert_eq!(from_str, from_value);
    }

    #[test]
    fn outgoing_from_str_or_value() {
        let v = json!({"jsonrpc":"2.0","result":{},"id":1});
        let from_str: Message = serde_json::from_str(&v.to_string()).unwrap();
        let from_value: Message = serde_json::from_value(v).unwrap();
        assert_eq!(from_str, from_value);
    }

    #[test]
    fn parses_incoming_message() {
        let server_request =
            json!({"jsonrpc":"2.0","method":"initialize","params":{"capabilities":{}},"id":0});
        let incoming = serde_json::from_value(server_request).unwrap();
        assert!(matches!(incoming, Message::Request(_)));

        let server_notif = json!({"jsonrpc":"2.0","method":"initialized","params":{}});
        let incoming = serde_json::from_value(server_notif).unwrap();
        assert!(matches!(incoming, Message::Request(_)));

        let client_request = json!({"jsonrpc":"2.0","id":0,"result":[null]});
        let incoming = serde_json::from_value(client_request).unwrap();
        assert!(matches!(incoming, Message::Response(_)));
    }

    #[test]
    fn parses_outgoing_message() {
        let client_request = json!({"jsonrpc":"2.0","method":"workspace/configuration","params":{"scopeUri":null,"section":"foo"},"id":0});
        let outgoing = serde_json::from_value(client_request).unwrap();
        assert!(matches!(outgoing, Message::Request(_)));

        let client_notif = json!({"jsonrpc":"2.0","method":"window/logMessage","params":{"message":"foo","type":0}});
        let outgoing = serde_json::from_value(client_notif).unwrap();
        assert!(matches!(outgoing, Message::Request(_)));

        let server_response = json!({"jsonrpc":"2.0","id":0,"result":[null]});
        let outgoing = serde_json::from_value(server_response).unwrap();
        assert!(matches!(outgoing, Message::Response(_)));
    }

    #[test]
    fn parses_invalid_server_request() {
        let unknown_method = json!({"jsonrpc":"2.0","method":"foo"});
        let incoming = serde_json::from_value(unknown_method).unwrap();
        assert!(matches!(incoming, Message::Request(_)));

        let unknown_method_with_id = json!({"jsonrpc":"2.0","method":"foo","id":0});
        let incoming = serde_json::from_value(unknown_method_with_id).unwrap();
        assert!(matches!(incoming, Message::Request(_)));

        let missing_method = json!({"jsonrpc":"2.0"});
        let incoming = serde_json::from_value(missing_method).unwrap();
        assert!(matches!(incoming, Message::Request(_)));

        let missing_method_with_id = json!({"jsonrpc":"2.0","id":0});
        let incoming = serde_json::from_value(missing_method_with_id).unwrap();
        assert!(matches!(incoming, Message::Request(_)));
    }

    #[test]
    fn accepts_null_request_id() {
        let request_id: Id = serde_json::from_value(json!(null)).unwrap();
        assert_eq!(request_id, Id::Null);
    }

    #[test]
    fn accepts_negative_integer_request_id() {
        let request_id: Id = serde_json::from_value(json!(-1)).unwrap();
        assert_eq!(request_id, Id::Number(-1));
    }
}
