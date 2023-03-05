use std::borrow::Cow;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use super::{Id, Version};

fn deserialize_some<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
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
    ///
    /// # Panics
    ///
    /// Panics if `params` could not be serialized into a [`serde_json::Value`]. Since the
    /// [`lsp_types::request::Request`] trait promises this invariant is upheld, this should never
    /// happen in practice (unless the trait was implemented incorrectly).
    pub(crate) fn from_request<R>(id: Id, params: R::Params) -> Self
    where
        R: lsp_types::request::Request,
    {
        Request {
            jsonrpc: Version,
            method: R::METHOD.into(),
            params: Some(serde_json::to_value(params).unwrap()),
            id: Some(id),
        }
    }

    /// Constructs a JSON-RPC notification from its corresponding LSP type.
    ///
    /// # Panics
    ///
    /// Panics if `params` could not be serialized into a [`serde_json::Value`]. Since the
    /// [`lsp_types::notification::Notification`] trait promises this invariant is upheld, this
    /// should never happen in practice (unless the trait was implemented incorrectly).
    pub(crate) fn from_notification<N>(params: N::Params) -> Self
    where
        N: lsp_types::notification::Notification,
    {
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
        use std::{io, str};

        struct WriterFormatter<'a, 'b: 'a> {
            inner: &'a mut Formatter<'b>,
        }

        impl<'a, 'b> io::Write for WriterFormatter<'a, 'b> {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                fn io_error<E>(_: E) -> io::Error {
                    // Error value does not matter because fmt::Display impl below just
                    // maps it to fmt::Error
                    io::Error::new(io::ErrorKind::Other, "fmt error")
                }
                let s = str::from_utf8(buf).map_err(io_error)?;
                self.inner.write_str(s).map_err(io_error)?;
                Ok(buf.len())
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let mut w = WriterFormatter { inner: f };
        serde_json::to_writer(&mut w, self).map_err(|_| fmt::Error)
    }
}

impl FromStr for Request {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(s)
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
