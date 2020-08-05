//! A subset of JSON-RPC types used by the Language Server Protocol.

pub use jsonrpc_core::params::Params;
pub use jsonrpc_core::request::{MethodCall, Notification};
pub use jsonrpc_core::response::{Failure, Output, Success};

pub use self::error::{Error, ErrorCode};

use std::fmt::{self, Debug, Display, Formatter};

use serde::de::{self, Deserializer};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

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
