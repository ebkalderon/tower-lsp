//! A subset of JSON-RPC types used by the Language Server Protocol.

#[doc(hidden)]
pub use serde_json::json;

pub(crate) use self::error::not_initialized_error;
pub use self::error::{Error, ErrorCode, Result};
pub use self::params::{to_params, Params};
pub use self::request::{Request, RequestBuilder};
pub use self::response::Response;
pub(crate) use self::router::Router;
pub use self::router::{FromParams, IntoResponse, Method};

use std::borrow::Cow;
use std::fmt::{self, Debug, Display, Formatter};

use lsp_types::NumberOrString;
use serde::de::{self, Deserializer};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

mod error;
mod params;
mod request;
mod response;
mod router;

/// A unique ID used to correlate requests and responses together.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Id {
    /// Numeric ID.
    Number(i64),
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

fn deserialize_opt_id<'de, D>(deserializer: D) -> std::result::Result<Option<Option<Id>>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde_json::Value;
    match Value::deserialize(deserializer)? {
        Value::Number(v) => Ok(Some(v.as_i64().map(Id::Number))),
        Value::String(v) => Ok(Some(Some(Id::String(v)))),
        _ => Ok(Some(None)),
    }
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
    /// An invalid JSON-RPC message.
    Invalid {
        /// The `id` field of the message, if detected.
        ///
        /// This field may be any of the following values:
        ///
        /// * `Some(Some(_))` if a field named `id` is present and contains an [`Id`].
        /// * `Some(None)` if a field named `id` is present, but it is not an [`Id`].
        /// * `None` if no field named `id` is present.
        ///
        /// If this message is a JSON object with some field named `id`, regardless of type, we
        /// should respond with an "invalid request" error. Otherwise, assume this is <TODO>
        ///
        /// TODO: We need to distinguish between an invalid JSON-RPC message and a failed
        /// notification somehow. Remember: we must not respond to failed notifications.
        #[serde(default, deserialize_with = "deserialize_opt_id")]
        id: Option<Option<Id>>,
    },
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
    fn accepts_negative_integer_request_id() {
        let request_id: Id = serde_json::from_value(json!(-1)).unwrap();
        assert_eq!(request_id, Id::Number(-1));
    }
}
