//! Encoder and decoder for Language Server Protocol messages.

use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io::{Error as IoError, Write};
use std::marker::PhantomData;
use std::str::Utf8Error;

use bytes::buf::BufMut;
use bytes::{Buf, BytesMut};
use log::trace;
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
    MissingHeader,
    /// The length value in the `Content-Length` header is invalid.
    InvalidLength,
    /// Request contains invalid UTF8.
    Utf8(Utf8Error),
}

impl Display for ParseError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match *self {
            ParseError::Body(ref e) => write!(f, "unable to parse JSON body: {}", e),
            ParseError::Encode(ref e) => write!(f, "failed to encode response: {}", e),
            ParseError::Httparse(ref e) => write!(f, "failed to parse headers: {}", e),
            ParseError::InvalidLength => write!(f, "unable to parse content length"),
            ParseError::MissingHeader => write!(f, "missing required `Content-Length` header"),
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
    _marker: PhantomData<T>,
}

impl<T> Default for LanguageServerCodec<T> {
    fn default() -> Self {
        LanguageServerCodec {
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
        let mut http_headers_err = None;
        let mut http_headers_len = None;
        let mut http_content_len = None::<usize>;

        // Placeholder used for parsing headers into
        let dst = &mut [httparse::EMPTY_HEADER; 2];

        // Parse the headers and try to extract values
        match httparse::parse_headers(src, dst) {
            // A complete set of headers was parsed succesfully
            Ok(httparse::Status::Complete((headers_len, headers))) => {
                // If some headers were parsed successefully, set the headers length
                http_headers_len = Some(headers_len);
                // If the "Content-Length" header exists, parse the value as a `usize`.
                if let Some(header) = headers.iter().find(|h| h.name == "Content-Length") {
                    match std::str::from_utf8(header.value) {
                        Ok(content_len) => match content_len.parse() {
                            // Successfully set `content_len` from the parsed "Content-Length"
                            // value.
                            Ok(content_len) => http_content_len = Some(content_len),
                            // If there was an error parsing the "Content-Length" UTF-8 as a
                            // `usize`, return the error.
                            Err(_) => {
                                src.advance(headers_len);
                                return Err(ParseError::InvalidLength);
                            }
                        },
                        // If there was an error parsing the "Content-Length" value as UTF-8,
                        // return the error.
                        Err(err) => {
                            src.advance(headers_len);
                            return Err(ParseError::Utf8(err));
                        }
                    }
                }
            }
            // No errors occurred during parsing yet but no complete set of headers were parsed
            Ok(httparse::Status::Partial) => return Ok(None),
            // An error occurred during parsing of the headers
            Err(err) => {
                http_headers_err = Some(err);
            }
        }

        // If "Content-Length" has been parsed
        if let (Some(headers_len), Some(content_len)) = (http_headers_len, http_content_len) {
            let message_len = headers_len + content_len;

            // Source doesn't contain the full content yet so return and wait for more input
            if src.len() < message_len {
                return Ok(None);
            }

            // Parse the JSON-RPC message bytes as JSON
            let message = &src[headers_len..message_len];
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

        // else if headers were parsed but "Content-Length" wasn't found
        } else {
            // Maybe there are garbage bytes so try to scan ahead for another "Content-Length"
            if let Some(offset) = memchr::memmem::find(src, b"Content-Length") {
                src.advance(offset);
            }

            // Handle the conditions that caused decoding to fail
            if let Some(err) = http_headers_err {
                // There was an error parsing the headers
                Err(ParseError::Httparse(err))
            } else {
                // There was no "Content-Length" header found
                Err(ParseError::MissingHeader)
            }
        }
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
        let encoded = encode_message(None, &decoded);

        let mut codec = LanguageServerCodec::default();
        let mut buffer = BytesMut::new();
        let item: Value = serde_json::from_str(&decoded).unwrap();
        codec.encode(item, &mut buffer).unwrap();
        assert_eq!(buffer, BytesMut::from(encoded.as_str()));

        let mut buffer = BytesMut::from(encoded.as_str());
        let message = codec.decode(&mut buffer).unwrap();
        let decoded = serde_json::from_str(&decoded).unwrap();
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
        let decoded: Value = serde_json::from_str(&decoded).unwrap();
        assert_eq!(message, Some(decoded));
    }

    #[test]
    fn decodes_zero_length_message() {
        let content_type = "Content-Type: application/vscode-jsonrpc; charset=utf-8";
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
            Err(ParseError::MissingHeader) => {}
            other => panic!("expected `Err(ParseError::MissingHeader)`, got {:?}", other),
        }

        let message: Option<Value> = codec.decode(&mut buffer).unwrap();
        let first_valid = serde_json::from_str(&decoded).unwrap();
        assert_eq!(message, Some(first_valid));

        match codec.decode(&mut buffer) {
            Err(ParseError::InvalidLength) => {}
            other => panic!("expected `Err(ParseError::InvalidLength)`, got {:?}", other),
        }

        let message = codec.decode(&mut buffer).unwrap();
        let second_valid = serde_json::from_str(&decoded).unwrap();
        assert_eq!(message, Some(second_valid));

        let message = codec.decode(&mut buffer).unwrap();
        assert_eq!(message, None);
    }
}
