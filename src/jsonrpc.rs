//! A subset of JSON-RPC types used by the Language Server Protocol.

pub use self::error::{Error, ErrorCode};
pub use self::router::{FromParams, IntoResponse, Method};

pub(crate) use self::router::Router;

use std::borrow::Cow;
use std::fmt::{self, Debug, Display, Formatter};

use lsp_types::NumberOrString;
use serde::de::{self, Deserializer};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod error;
mod router;

/// A specialized [`Result`] error type for JSON-RPC handlers.
///
/// [`Result`]: enum@std::result::Result
pub type Result<T> = std::result::Result<T, Error>;

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

fn deserialize_some<'de, T, D>(deserializer: D) -> std::result::Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// A JSON-RPC request or notification.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Request {
    jsonrpc: Version,
    #[serde(default)]
    method: Cow<'static, str>,
    #[serde(default, deserialize_with = "deserialize_some")]
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
    #[serde(default, deserialize_with = "deserialize_some")]
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Id>,
}

impl Request {
    /// Starts building a JSON-RPC method call.
    ///
    /// Returns a `RequestBuilder`, which allows setting the `params` field or adding a request ID.
    pub fn build<M>(method: M) -> RequestBuilder
    where
        M: Into<Cow<'static, str>>,
    {
        RequestBuilder {
            method: method.into(),
            params: None,
            id: None,
        }
    }

    /// Constructs a JSON-RPC request from its corresponding LSP type.
    pub(crate) fn from_request<R>(id: Id, params: R::Params) -> Self
    where
        R: lsp_types::request::Request,
    {
        // Since `R::Params` come from the `lsp-types` crate and validity is enforced via the
        // `Request` trait, the `unwrap()` call below should never fail.
        Request {
            jsonrpc: Version,
            method: R::METHOD.into(),
            params: Some(serde_json::to_value(params).unwrap()),
            id: Some(id),
        }
    }

    /// Constructs a JSON-RPC notification from its corresponding LSP type.
    pub(crate) fn from_notification<N>(params: N::Params) -> Self
    where
        N: lsp_types::notification::Notification,
    {
        // Since `N::Params` comes from the `lsp-types` crate and validity is enforced via the
        // `Notification` trait, the `unwrap()` call below should never fail.
        Request {
            jsonrpc: Version,
            method: N::METHOD.into(),
            params: Some(serde_json::to_value(params).unwrap()),
            id: None,
        }
    }

    /// Returns the name of the method to be invoked.
    pub fn method(&self) -> &str {
        self.method.as_ref()
    }

    /// Returns the unique ID of this request, if present.
    pub fn id(&self) -> Option<&Id> {
        self.id.as_ref()
    }

    /// Returns the `params` field, if present.
    pub fn params(&self) -> Option<&Value> {
        self.params.as_ref()
    }

    /// Splits this request into the method name, request ID, and the `params` field, if present.
    pub fn into_parts(self) -> (Cow<'static, str>, Option<Id>, Option<Value>) {
        (self.method, self.id, self.params)
    }
}

impl Display for Request {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut w = WriterFormatter { inner: f };
        serde_json::to_writer(&mut w, self).map_err(|_| fmt::Error)
    }
}

struct WriterFormatter<'a, 'b: 'a> {
    inner: &'a mut Formatter<'b>,
}

impl<'a, 'b> std::io::Write for WriterFormatter<'a, 'b> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        fn io_error<E>(_: E) -> std::io::Error {
            // Error value does not matter because fmt::Display impl below just
            // maps it to fmt::Error
            std::io::Error::new(std::io::ErrorKind::Other, "fmt error")
        }
        let s = std::str::from_utf8(buf).map_err(io_error)?;
        self.inner.write_str(s).map_err(io_error)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// A builder to construct the properties of a `Request`.
///
/// To construct a `RequestBuilder`, refer to [`Request::build`].
#[derive(Debug)]
pub struct RequestBuilder {
    method: Cow<'static, str>,
    params: Option<Value>,
    id: Option<Id>,
}

impl RequestBuilder {
    /// Sets the `id` member of the request to the given value.
    ///
    /// If this method is not called, the resulting `Request` will be assumed to be a notification.
    pub fn id<I: Into<Id>>(mut self, id: I) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Sets the `params` member of the request to the given value.
    ///
    /// This member is omitted from the request by default.
    pub fn params<V: Into<Value>>(mut self, params: V) -> Self {
        self.params = Some(params.into());
        self
    }

    /// Constructs the JSON-RPC request and returns it.
    pub fn finish(self) -> Request {
        Request {
            jsonrpc: Version,
            method: self.method,
            params: self.params,
            id: self.id,
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Version;

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
