//! Encoder and decoder for Language Server Protocol messages.

use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io::{Error as IoError, Write};
use std::marker::PhantomData;
use std::num::ParseIntError;
use std::str::Utf8Error;

use bytes::buf::BufMut;
use bytes::{Buf, BytesMut};
use log::{trace, warn};
use serde::{de::DeserializeOwned, Serialize};

#[cfg(feature = "runtime-agnostic")]
use async_codec_lite::{Decoder, Encoder};
#[cfg(feature = "runtime-tokio")]
use tokio_util::codec::{Decoder, Encoder};

/// Errors that can occur when processing an LSP request.
#[derive(Debug)]
pub enum ParseError {
    /// Failed to parse the JSON body.
    Body(serde_json::Error),
    /// Failed to encode the response.
    Encode(IoError),
    /// Failed to parse headers.
    Httparse(httparse::Error),
    /// Request lacks the required `Content-Length` header.
    MissingContentLength,
    /// The length value in the `Content-Length` header is invalid.
    InvalidContentLength(ParseIntError),
    /// Request contains invalid UTF8.
    Utf8(Utf8Error),
}

impl Display for ParseError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match *self {
            ParseError::Body(ref e) => write!(f, "unable to parse JSON body: {}", e),
            ParseError::Encode(ref e) => write!(f, "failed to encode response: {}", e),
            ParseError::Httparse(ref e) => write!(f, "failed to parse headers: {}", e),
            ParseError::InvalidContentLength(ref e) => {
                write!(f, "unable to parse content length: {}", e)
            }
            ParseError::MissingContentLength => {
                write!(f, "missing required `Content-Length` header")
            }
            ParseError::Utf8(ref e) => write!(f, "request contains invalid UTF8: {}", e),
        }
    }
}

impl Error for ParseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match *self {
            ParseError::Body(ref e) => Some(e),
            ParseError::Encode(ref e) => Some(e),
            ParseError::Utf8(ref e) => Some(e),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for ParseError {
    fn from(error: serde_json::Error) -> Self {
        ParseError::Body(error)
    }
}

impl From<IoError> for ParseError {
    fn from(error: IoError) -> Self {
        ParseError::Encode(error)
    }
}

impl From<Utf8Error> for ParseError {
    fn from(error: Utf8Error) -> Self {
        ParseError::Utf8(error)
    }
}

/// Encodes and decodes Language Server Protocol messages.
#[derive(Clone, Debug)]
pub struct LanguageServerCodec<T> {
    message_len: Option<usize>,
    _marker: PhantomData<T>,
}

impl<T> LanguageServerCodec<T> {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

impl<T> Default for LanguageServerCodec<T> {
    fn default() -> Self {
        LanguageServerCodec {
            message_len: None,
            _marker: PhantomData,
        }
    }
}

#[cfg(feature = "runtime-agnostic")]
impl<T: Serialize> Encoder for LanguageServerCodec<T> {
    type Item = T;
    type Error = ParseError;

    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let msg = serde_json::to_string(&item)?;
        trace!("-> {}", msg);

        // Reserve just enough space to hold the `Content-Length: ` and `\r\n\r\n` constants,
        // the length of the message, and the message body.
        dst.reserve(msg.len() + number_of_digits(msg.len()) + 20);
        let mut writer = dst.writer();
        write!(writer, "Content-Length: {}\r\n\r\n{}", msg.len(), msg)?;
        writer.flush()?;

        Ok(())
    }
}

#[cfg(feature = "runtime-tokio")]
impl<T: Serialize> Encoder<T> for LanguageServerCodec<T> {
    type Error = ParseError;

    fn encode(&mut self, item: T, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let msg = serde_json::to_string(&item)?;
        trace!("-> {}", msg);

        // Reserve just enough space to hold the `Content-Length: ` and `\r\n\r\n` constants,
        // the length of the message, and the message body.
        dst.reserve(msg.len() + number_of_digits(msg.len()) + 20);
        let mut writer = dst.writer();
        write!(writer, "Content-Length: {}\r\n\r\n{}", msg.len(), msg)?;
        writer.flush()?;

        Ok(())
    }
}

#[inline]
fn number_of_digits(mut n: usize) -> usize {
    let mut num_digits = 0;

    while n > 0 {
        n /= 10;
        num_digits += 1;
    }

    num_digits
}

impl<T: DeserializeOwned> Decoder for LanguageServerCodec<T> {
    type Item = T;
    type Error = ParseError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // If message length is known and source buffer doesn't contain the full message content yet
        if src.len() < self.message_len.unwrap_or_default() {
            return Ok(None);
        }

        let mut http_headers_err = Option::<ParseError>::default();

        // If message length has not been parsed from "Content-Length" header yet
        if self.message_len.is_none() {
            // Placeholder used for parsing headers into
            let dst = &mut [httparse::EMPTY_HEADER; 2];

            // Parse the headers and try to extract values
            match httparse::parse_headers(src, dst) {
                // A complete set of headers was parsed succesfully
                Ok(httparse::Status::Complete((headers_len, headers))) => {
                    // Process the parsed headers
                    for header in headers {
                        match header.name {
                            // Process a "Content-Length" header and extract the length value
                            "Content-Length" => match std::str::from_utf8(header.value) {
                                Ok(content_len) => match content_len.parse::<usize>() {
                                    Ok(content_len) => {
                                        self.message_len = Some(content_len);
                                    }
                                    Err(err) => {
                                        http_headers_err =
                                            Some(ParseError::InvalidContentLength(err));
                                        break;
                                    }
                                },
                                Err(err) => {
                                    http_headers_err = Some(ParseError::Utf8(err));
                                }
                            },
                            // Process a "Content-Type" header and just check that the value is of an expected value
                            "Content-Type" => {
                                if header.value != b"application/vscode-jsonrpc; charset=utf-8" {
                                    warn!(
                                        "encountered unexpected Content-Type value: {:#?}",
                                        std::str::from_utf8(header.value)
                                    );
                                }
                            }
                            // Otherwise warn about unsupported headers
                            _ => {
                                warn!(
                                    "encountered http header unsupported by LSP spec: {:#?}",
                                    header
                                );
                            }
                        }
                    }
                    // If "Content-Length" was found (either with a valid or invalid value) advance beyond headers
                    if self.message_len.is_some()
                        || matches!(http_headers_err, Some(ParseError::InvalidContentLength(_)))
                    {
                        // Advance the buffer beyond the http headers
                        src.advance(headers_len);
                    }
                }
                // No errors occurred during parsing yet but no complete set of headers were parsed
                Ok(httparse::Status::Partial) => return Ok(None),
                // An error occurred during parsing of the headers
                Err(err) => {
                    http_headers_err = Some(ParseError::Httparse(err));
                }
            }
        }

        // If message length is known and source buffer contains at least the full message content
        let result = if let Some(message_len) = self.message_len {
            // Parse the JSON-RPC message bytes as JSON
            let message = &src[..message_len];
            let message = std::str::from_utf8(message)?;

            trace!("<- {}", message);

            // Deserialize the JSON-RPC message data
            let data = {
                // For zero-length data just return None
                if message.is_empty() {
                    Ok(None)
                // Otherwise deserialize data as JSON text
                } else {
                    match serde_json::from_str(message) {
                        Ok(parsed) => Ok(Some(parsed)),
                        Err(err) => Err(err.into()),
                    }
                }
            };

            // Advance the buffer
            src.advance(message_len);

            // Return the deserialized data
            data
        // Otherwise there was an error parsing the "Content-Length" header or the header was missing
        } else {
            // Advance the buffer
            src.advance(memchr::memmem::find(src, b"Content-Length").unwrap_or_default());

            // Either there was an error parsing the "Content-Length" header...
            if let Some(err) = http_headers_err {
                Err(err)
            // ... or the "Content-Length" header was missing
            } else {
                Err(ParseError::MissingContentLength)
            }
        };

        self.reset();

        result
    }
}

#[cfg(test)]
mod tests {
    use bytes::BytesMut;
    use serde_json::Value;

    use super::*;

    fn encode_message(content_type: Option<&str>, message: &str) -> String {
        let content_type = content_type
            .map(|ty| format!("\r\nContent-Type: {}", ty))
            .unwrap_or_default();

        format!(
            "Content-Length: {}{}\r\n\r\n{}",
            message.len(),
            content_type,
            message
        )
    }

    #[test]
    fn encode_and_decode() {
        let decoded = r#"{"jsonrpc":"2.0","method":"exit"}"#;
        let encoded = encode_message(None, decoded);

        let mut codec = LanguageServerCodec::default();
        let mut buffer = BytesMut::new();
        let item: Value = serde_json::from_str(decoded).unwrap();
        codec.encode(item, &mut buffer).unwrap();
        assert_eq!(buffer, BytesMut::from(encoded.as_str()));

        let mut buffer = BytesMut::from(encoded.as_str());
        let message = codec.decode(&mut buffer).unwrap();
        let decoded = serde_json::from_str(decoded).unwrap();
        assert_eq!(message, Some(decoded));
    }

    #[test]
    fn decodes_optional_content_type() {
        let decoded = r#"{"jsonrpc":"2.0","method":"exit"}"#;
        let content_type = "application/vscode-jsonrpc; charset=utf-8";
        let encoded = encode_message(Some(content_type), decoded);

        let mut codec = LanguageServerCodec::default();
        let mut buffer = BytesMut::from(encoded.as_str());
        let message = codec.decode(&mut buffer).unwrap();
        let decoded: Value = serde_json::from_str(decoded).unwrap();
        assert_eq!(message, Some(decoded));
    }

    #[test]
    fn decodes_zero_length_message() {
        let content_type = "application/vscode-jsonrpc; charset=utf-8";
        let encoded = encode_message(Some(content_type), "");

        let mut codec = LanguageServerCodec::default();
        let mut buffer = BytesMut::from(encoded.as_str());
        let message: Option<Value> = codec.decode(&mut buffer).unwrap();
        assert_eq!(message, None);
    }

    #[test]
    fn recovers_from_parse_error() {
        let decoded = r#"{"jsonrpc":"2.0","method":"exit"}"#;
        let encoded = encode_message(None, decoded);
        let mixed = format!("foobar{}Content-Length: foobar\r\n\r\n{}", encoded, encoded);

        let mut codec = LanguageServerCodec::default();
        let mut buffer = BytesMut::from(mixed.as_str());

        match codec.decode(&mut buffer) {
            Err(ParseError::MissingContentLength) => {}
            other => panic!(
                "expected `Err(ParseError::MissingContentLength)`, got {:?}",
                other
            ),
        }

        let message: Option<Value> = codec.decode(&mut buffer).unwrap();
        let first_valid = serde_json::from_str(decoded).unwrap();
        assert_eq!(message, Some(first_valid));

        match codec.decode(&mut buffer) {
            Err(ParseError::InvalidContentLength(_)) => {}
            other => panic!(
                "expected `Err(ParseError::InvalidContentLength)`, got {:?}",
                other
            ),
        }

        let message = codec.decode(&mut buffer).unwrap();
        let second_valid = serde_json::from_str(decoded).unwrap();
        assert_eq!(message, Some(second_valid));

        let message = codec.decode(&mut buffer).unwrap();
        assert_eq!(message, None);
    }
}
