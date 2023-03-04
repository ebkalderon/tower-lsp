use std::convert::TryFrom;
use std::fmt::{self, Display, Formatter};

use serde::de::DeserializeOwned;
use serde::{de, Deserialize, Serialize};
use serde_json::{Map, Value};

/// Parameters sent with an incoming JSON-RPC request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(into = "Value", try_from = "Value")]
pub enum Params {
    /// Represents a by-position `"params"` value.
    Array(Vec<Value>),
    /// Represents a by-name `"params"` value.
    Object(Map<String, Value>),
}

impl Params {
    /// Attempts to parse these parameters into type `T`.
    ///
    /// This conversion can fail if the structure of the `Params` does not match the structure
    /// expected by `T`, for example if `T` is a struct type but the `Params` contains an array. It
    /// can also fail if the structure is correct but `T`’s implementation of `Deserialize` decides
    /// that something is wrong with the data, for example required struct fields are missing from
    /// the JSON map or some number is too big to fit in the expected primitive type.
    pub fn parse<T>(self) -> serde_json::Result<T>
    where
        T: DeserializeOwned,
    {
        serde_json::from_value(self.into())
    }
}

impl Display for Params {
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

impl<T: Into<Value>> From<Vec<T>> for Params {
    fn from(v: Vec<T>) -> Self {
        Params::Array(v.into_iter().map(Into::into).collect())
    }
}

impl<'a, T: Clone + Into<Value>> From<&'a [T]> for Params {
    fn from(v: &'a [T]) -> Self {
        Params::Array(v.iter().cloned().map(Into::into).collect())
    }
}

impl<K: Into<String>, V: Into<Value>> FromIterator<(K, V)> for Params {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        Params::Object(
            iter.into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        )
    }
}

impl Into<Value> for Params {
    fn into(self) -> Value {
        match self {
            Params::Array(v) => Value::Array(v),
            Params::Object(v) => Value::Object(v),
        }
    }
}

impl TryFrom<Value> for Params {
    type Error = serde_json::Error;

    fn try_from(v: Value) -> Result<Self, Self::Error> {
        match v {
            Value::Array(v) => Ok(Params::Array(v)),
            Value::Object(v) => Ok(Params::Object(v)),
            _ => Err(de::Error::custom(
                "expected `params` to be an array or object",
            )),
        }
    }
}

impl PartialEq<Value> for Params {
    fn eq(&self, other: &Value) -> bool {
        match (self, other) {
            (Params::Array(p), Value::Array(v)) => p.eq(v),
            (Params::Object(p), Value::Object(v)) => p.eq(v),
            _ => false,
        }
    }
}

impl PartialEq<Params> for Value {
    fn eq(&self, other: &Params) -> bool {
        match (self, other) {
            (Value::Array(v), Params::Array(p)) => v.eq(p),
            (Value::Object(v), Params::Object(p)) => v.eq(p),
            _ => false,
        }
    }
}

/// Converts a `T` into [`Params`] for a JSON-RPC [`Request`](super::Request).
///
/// Returns `Err` if `T`'s implementation of `Serialize` decides to fail, or `T` contains a map
/// with non-string keys.
pub fn to_params<T>(value: T) -> serde_json::Result<Params>
where
    T: Serialize,
{
    serde_json::to_value(value).and_then(Params::try_from)
}

/// Constructs a [`Params`] from a JSON literal.
///
/// This macro behaves identically to [`serde_json::json!`], except it only accepts an array or
/// object literal at the top level.
#[macro_export]
macro_rules! params {
    ([ $($elems:tt)* ]) => {
        $crate::params_inner!([ $($elems)* ])
    };

    ({ $($members:tt)* }) => {
        $crate::params_inner!({ $($members)* })
    };

}

#[doc(hidden)]
#[macro_export]
macro_rules! params_inner {
    ($($tokens:tt)+) => {
        match <$crate::jsonrpc::Params as ::std::convert::TryFrom<_>>::try_from($crate::jsonrpc::json!($($tokens)*)) {
            Ok(params) => params,
            Err(_) => unreachable!(),
        }
    }
}
