//! Encoder and decoder for Language Server Protocol messages.

use std::error::Error;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::io::{Error as IoError, Write};
use std::str::{self, Utf8Error};

use bytes::buf::ext::BufMutExt;
use bytes::{Buf, BytesMut};
use nom::branch::alt;
use nom::bytes::streaming::{is_not, tag};
use nom::character::streaming::{char, crlf, digit1, space0};
use nom::combinator::{map_res, opt};
use nom::error::ErrorKind;
use nom::multi::length_data;
use nom::sequence::{delimited, terminated, tuple};
use nom::{Err, IResult, Needed};
use tokio_util::codec::{Decoder, Encoder};

/// Errors that can occur when processing an LSP request.
#[derive(Debug)]
pub enum ParseError {
    /// Request lacks the required `Content-Length` header.
    MissingHeader,
    /// The length value in the `Content-Length` header is invalid.
    InvalidLength,
    /// The media type in the `Content-Type` header is invalid.
    InvalidType,
    /// Failed to encode the response.
    Encode(IoError),
    /// Request contains invalid UTF8.
    Utf8(Utf8Error),
}

impl Display for ParseError {
    fn fmt(&self, fmt: &mut Formatter) -> FmtResult {
        match *self {
            ParseError::MissingHeader => write!(fmt, "missing required `Content-Length` header"),
            ParseError::InvalidLength => write!(fmt, "unable to parse content length"),
            ParseError::InvalidType => write!(fmt, "unable to parse content type"),
            ParseError::Encode(ref e) => write!(fmt, "failed to encode response: {}", e),
            ParseError::Utf8(ref e) => write!(fmt, "request contains invalid UTF8: {}", e),
        }
    }
}

impl Error for ParseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match *self {
            ParseError::Encode(ref e) => Some(e),
            ParseError::Utf8(ref e) => Some(e),
            _ => None,
        }
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
///
/// # Encoding
///
/// If the message length is zero, then the codec will skip encoding the message.
#[derive(Clone, Debug, Default)]
pub struct LanguageServerCodec {
    remaining_msg_bytes: usize,
}

impl Encoder<String> for LanguageServerCodec {
    type Error = ParseError;

    fn encode(&mut self, item: String, dst: &mut BytesMut) -> Result<(), Self::Error> {
        if !item.is_empty() {
            // Reserve just enough space to hold the `Content-Length: ` and `\r\n\r\n` constants,
            // the length of the message, and the message body.
            dst.reserve(item.len() + number_of_digits(item.len()) + 20);
            let mut writer = dst.writer();
            write!(writer, "Content-Length: {}\r\n\r\n{}", item.len(), item)?;
            writer.flush()?;
        }

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

impl Decoder for LanguageServerCodec {
    type Item = String;
    type Error = ParseError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if self.remaining_msg_bytes > src.len() {
            return Ok(None);
        }

        let (message, len) = match parse_message(&src) {
            Ok((remaining, message)) => (
                str::from_utf8(message)?.to_string(),
                src.len() - remaining.len(),
            ),
            Err(Err::Incomplete(Needed::Size(min))) => {
                self.remaining_msg_bytes = min;
                return Ok(None);
            }
            Err(Err::Incomplete(_)) => {
                return Ok(None);
            }
            Err(Err::Error((_, err))) | Err(Err::Failure((_, err))) => match err {
                ErrorKind::Digit | ErrorKind::MapRes => return Err(ParseError::InvalidLength),
                ErrorKind::Char | ErrorKind::IsNot => return Err(ParseError::InvalidType),
                _ => return Err(ParseError::MissingHeader),
            },
        };

        src.advance(len);
        self.remaining_msg_bytes = 0;

        Ok(Some(message))
    }
}

fn parse_message(input: &[u8]) -> IResult<&[u8], &[u8]> {
    let content_len = delimited(tag("Content-Length: "), digit1, crlf);

    let utf8 = alt((tag("utf-8"), tag("utf8")));
    let charset = tuple((char(';'), space0, tag("charset="), utf8));
    let content_type = tuple((tag("Content-Type:"), is_not(";\r"), opt(charset), crlf));

    let header = terminated(terminated(content_len, opt(content_type)), crlf);
    let header = map_res(header, |s: &[u8]| str::from_utf8(s));
    let length = map_res(header, |s: &str| s.parse::<usize>());
    let message = length_data(length);

    message(input)
}

#[cfg(test)]
mod tests {
    use bytes::BytesMut;

    use super::*;

    #[test]
    fn encode_and_decode() {
        let decoded = r#"{"jsonrpc":"2.0","method":"exit"}"#.to_string();
        let encoded = format!("Content-Length: {}\r\n\r\n{}", decoded.len(), decoded);

        let mut codec = LanguageServerCodec::default();
        let mut buffer = BytesMut::new();
        codec.encode(decoded.clone(), &mut buffer).unwrap();
        assert_eq!(buffer, BytesMut::from(encoded.as_str()));

        let mut buffer = BytesMut::from(encoded.as_str());
        let message = codec.decode(&mut buffer).unwrap();
        assert_eq!(message, Some(decoded));
    }

    #[test]
    fn skip_encoding_empty_message() {
        let mut codec = LanguageServerCodec::default();
        let mut buffer = BytesMut::new();
        codec.encode("".to_string(), &mut buffer).unwrap();
        assert_eq!(buffer, BytesMut::new());
    }

    #[test]
    fn decodes_optional_content_type() {
        let decoded = r#"{"jsonrpc":"2.0","method":"exit"}"#.to_string();
        let content_len = format!("Content-Length: {}", decoded.len());
        let content_type = "Content-Type: application/vscode-jsonrpc; charset=utf-8".to_string();
        let encoded = format!("{}\r\n{}\r\n\r\n{}", content_len, content_type, decoded);

        let mut codec = LanguageServerCodec::default();
        let mut buffer = BytesMut::from(encoded.as_str());
        let message = codec.decode(&mut buffer).unwrap();
        assert_eq!(message, Some(decoded));
    }
}
