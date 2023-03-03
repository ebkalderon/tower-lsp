//! Encoder and decoder for Language Server Protocol messages.

use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io::{Error as IoError, Write};
use std::marker::PhantomData;
use std::num::ParseIntError;
use std::str::Utf8Error;

use bytes::buf::BufMut;
use bytes::{Buf, BytesMut};
use memchr::memmem;
use serde::{de::DeserializeOwned, Serialize};
use tracing::{trace, warn};

#[cfg(feature = "runtime-agnostic")]
use async_codec_lite::{Decoder, Encoder};
#[cfg(feature = "runtime-tokio")]
use tokio_util::codec::{Decoder, Encoder};

/// Errors that can occur when processing an LSP message.
#[derive(Debug)]
pub enum ParseError {
    /// Failed to parse the JSON body.
    Body(serde_json::Error),
    /// Failed to encode the response.
    Encode(IoError),
    /// Failed to parse headers.
    Headers(httparse::Error),
    /// The media type in the `Content-Type` header is invalid.
    InvalidContentType,
    /// The length value in the `Content-Length` header is invalid.
    InvalidContentLength(ParseIntError),
    /// Request lacks the required `Content-Length` header.
    MissingContentLength,
    /// Request contains invalid UTF8.
    Utf8(Utf8Error),
}

impl Display for ParseError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match *self {
            ParseError::Body(ref e) => write!(f, "unable to parse JSON body: {e}"),
            ParseError::Encode(ref e) => write!(f, "failed to encode response: {e}"),
            ParseError::Headers(ref e) => write!(f, "failed to parse headers: {e}"),
            ParseError::InvalidContentType => write!(f, "unable to parse content type"),
            ParseError::InvalidContentLength(ref e) => {
                write!(f, "unable to parse content length: {e}")
            }
            ParseError::MissingContentLength => {
                write!(f, "missing required `Content-Length` header")
            }
            ParseError::Utf8(ref e) => write!(f, "request contains invalid UTF8: {e}"),
        }
    }
}

impl Error for ParseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match *self {
            ParseError::Body(ref e) => Some(e),
            ParseError::Encode(ref e) => Some(e),
            ParseError::InvalidContentLength(ref e) => Some(e),
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

impl From<httparse::Error> for ParseError {
    fn from(error: httparse::Error) -> Self {
        ParseError::Headers(error)
    }
}

impl From<ParseIntError> for ParseError {
    fn from(error: ParseIntError) -> Self {
        ParseError::InvalidContentLength(error)
    }
}

impl From<Utf8Error> for ParseError {
    fn from(error: Utf8Error) -> Self {
        ParseError::Utf8(error)
    }
}

/// Encodes and decodes Language Server Protocol messages.
pub struct LanguageServerCodec<T> {
    content_len: Option<usize>,
    _marker: PhantomData<T>,
}

impl<T> Default for LanguageServerCodec<T> {
    fn default() -> Self {
        LanguageServerCodec {
            content_len: None,
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
        if let Some(content_len) = self.content_len {
            if src.len() < content_len {
                return Ok(None);
            }

            let bytes = &src[..content_len];
            let message = std::str::from_utf8(bytes)?;

            let result = if message.is_empty() {
                Ok(None)
            } else {
                trace!("<- {}", message);
                match serde_json::from_str(message) {
                    Ok(parsed) => Ok(Some(parsed)),
                    Err(err) => Err(err.into()),
                }
            };

            src.advance(content_len);
            self.content_len = None; // Reset state in preparation for parsing next message.

            result
        } else {
            let mut dst = [httparse::EMPTY_HEADER; 2];

            let (headers_len, headers) = match httparse::parse_headers(src, &mut dst)? {
                httparse::Status::Complete(output) => output,
                httparse::Status::Partial => return Ok(None),
            };

            match decode_headers(headers) {
                Ok(content_len) => {
                    src.advance(headers_len);
                    self.content_len = Some(content_len);
                    self.decode(src) // Recurse right back in, now that `Content-Length` is known.
                }
                Err(err) => {
                    match err {
                        ParseError::MissingContentLength => {}
                        _ => src.advance(headers_len),
                    }

                    // Skip any garbage bytes by scanning ahead for another potential message.
                    src.advance(memmem::find(src, b"Content-Length").unwrap_or_default());
                    Err(err)
                }
            }
        }
    }
}

fn decode_headers(headers: &[httparse::Header<'_>]) -> Result<usize, ParseError> {
    let mut content_len = None;

    for header in headers {
        match header.name {
            "Content-Length" => {
                let string = std::str::from_utf8(header.value)?;
                let parsed_len = string.parse()?;
                content_len = Some(parsed_len);
            }
            "Content-Type" => {
                let string = std::str::from_utf8(header.value)?;
                let charset = string
                    .split(';')
                    .skip(1)
                    .map(|param| param.trim())
                    .find_map(|param| param.strip_prefix("charset="));

                match charset {
                    Some("utf-8") | Some("utf8") => {}
                    _ => return Err(ParseError::InvalidContentType),
                }
            }
            other => warn!("encountered unsupported header: {:?}", other),
        }
    }

    if let Some(content_len) = content_len {
        Ok(content_len)
    } else {
        Err(ParseError::MissingContentLength)
    }
}

#[cfg(test)]
mod tests {
    use bytes::BytesMut;
    use serde_json::Value;

    use super::*;

    macro_rules! assert_err {
        ($expression:expr, $($pattern:tt)+) => {
            match $expression {
                $($pattern)+ => (),
                ref e => panic!("expected `{}` but got `{:?}`", stringify!($($pattern)+), e),
            }
        }
    }

    fn encode_message(content_type: Option<&str>, message: &str) -> String {
        let content_type = content_type
            .map(|ty| format!("\r\nContent-Type: {ty}"))
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
        let decoded_: Value = serde_json::from_str(decoded).unwrap();
        assert_eq!(message, Some(decoded_));

        let content_type = "application/vscode-jsonrpc; charset=utf8";
        let encoded = encode_message(Some(content_type), decoded);

        let mut buffer = BytesMut::from(encoded.as_str());
        let message = codec.decode(&mut buffer).unwrap();
        let decoded_: Value = serde_json::from_str(decoded).unwrap();
        assert_eq!(message, Some(decoded_));

        let content_type = "application/vscode-jsonrpc; charset=invalid";
        let encoded = encode_message(Some(content_type), decoded);

        let mut buffer = BytesMut::from(encoded.as_str());
        assert_err!(
            codec.decode(&mut buffer),
            Err(ParseError::InvalidContentType)
        );

        let content_type = "application/vscode-jsonrpc";
        let encoded = encode_message(Some(content_type), decoded);

        let mut buffer = BytesMut::from(encoded.as_str());
        assert_err!(
            codec.decode(&mut buffer),
            Err(ParseError::InvalidContentType)
        );

        let content_type = "this-mime-should-be-ignored; charset=utf8";
        let encoded = encode_message(Some(content_type), decoded);

        let mut buffer = BytesMut::from(encoded.as_str());
        let message = codec.decode(&mut buffer).unwrap();
        let decoded_: Value = serde_json::from_str(decoded).unwrap();
        assert_eq!(message, Some(decoded_));
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
        let mixed = format!("foobar{encoded}Content-Length: foobar\r\n\r\n{encoded}");

        let mut codec = LanguageServerCodec::default();
        let mut buffer = BytesMut::from(mixed.as_str());
        assert_err!(
            codec.decode(&mut buffer),
            Err(ParseError::MissingContentLength)
        );

        let message: Option<Value> = codec.decode(&mut buffer).unwrap();
        let first_valid = serde_json::from_str(decoded).unwrap();
        assert_eq!(message, Some(first_valid));
        assert_err!(
            codec.decode(&mut buffer),
            Err(ParseError::InvalidContentLength(_))
        );

        let message = codec.decode(&mut buffer).unwrap();
        let second_valid = serde_json::from_str(decoded).unwrap();
        assert_eq!(message, Some(second_valid));

        let message = codec.decode(&mut buffer).unwrap();
        assert_eq!(message, None);
    }

    #[test]
    fn decodes_small_chunks() {
        let decoded = r#"{"jsonrpc":"2.0","method":"exit"}"#;
        let content_type = "application/vscode-jsonrpc; charset=utf-8";
        let encoded = encode_message(Some(content_type), decoded);

        let mut codec = LanguageServerCodec::default();
        let mut buffer = BytesMut::from(encoded.as_str());

        let rest = buffer.split_off(40);
        let message = codec.decode(&mut buffer).unwrap();
        assert_eq!(message, None);
        buffer.unsplit(rest);

        let rest = buffer.split_off(80);
        let message = codec.decode(&mut buffer).unwrap();
        assert_eq!(message, None);
        buffer.unsplit(rest);

        let rest = buffer.split_off(16);
        let message = codec.decode(&mut buffer).unwrap();
        assert_eq!(message, None);
        buffer.unsplit(rest);

        let decoded: Value = serde_json::from_str(decoded).unwrap();
        let message = codec.decode(&mut buffer).unwrap();
        assert_eq!(message, Some(decoded));
    }
}
