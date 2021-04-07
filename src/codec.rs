//! Encoder and decoder for Language Server Protocol messages.

use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io::{Error as IoError, Write};
use std::marker::PhantomData;
use std::str::{self, Utf8Error};

use bytes::buf::BufMut;
use bytes::{Buf, BytesMut};
use log::trace;
use nom::branch::alt;
use nom::bytes::streaming::{is_not, tag};
use nom::character::streaming::{char, crlf, digit1, space0};
use nom::combinator::{map_res, opt};
use nom::error::ErrorKind;
use nom::multi::length_data;
use nom::sequence::{delimited, terminated, tuple};
use nom::{Err, IResult, Needed};
use serde::{de::DeserializeOwned, Serialize};
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
    /// Failed to parse the JSON body.
    Body(serde_json::Error),
    /// Failed to encode the response.
    Encode(IoError),
    /// Request contains invalid UTF8.
    Utf8(Utf8Error),
}

impl Display for ParseError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match *self {
            ParseError::MissingHeader => write!(f, "missing required `Content-Length` header"),
            ParseError::InvalidLength => write!(f, "unable to parse content length"),
            ParseError::InvalidType => write!(f, "unable to parse content type"),
            ParseError::Body(ref e) => write!(f, "unable to parse JSON body: {}", e),
            ParseError::Encode(ref e) => write!(f, "failed to encode response: {}", e),
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
    remaining_msg_bytes: usize,
    _marker: PhantomData<T>,
}

impl<T> Default for LanguageServerCodec<T> {
    fn default() -> Self {
        LanguageServerCodec {
            remaining_msg_bytes: 0,
            _marker: PhantomData,
        }
    }
}

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
        if self.remaining_msg_bytes > src.len() {
            return Ok(None);
        }

        let (msg, len) = match parse_message(src) {
            Ok((remaining, msg)) => (str::from_utf8(msg), src.len() - remaining.len()),
            Err(Err::Incomplete(Needed::Size(min))) => {
                self.remaining_msg_bytes = min;
                return Ok(None);
            }
            Err(Err::Incomplete(_)) => {
                return Ok(None);
            }
            Err(Err::Error((_, err))) | Err(Err::Failure((_, err))) => loop {
                use ParseError::*;
                match parse_message(src) {
                    Err(_) if !src.is_empty() => src.advance(1),
                    _ => match err {
                        ErrorKind::Digit | ErrorKind::MapRes => return Err(InvalidLength),
                        ErrorKind::Char | ErrorKind::IsNot => return Err(InvalidType),
                        _ => return Err(MissingHeader),
                    },
                }
            },
        };

        let result = match msg {
            Err(err) => Err(err.into()),
            Ok(msg) => {
                trace!("<- {}", msg);
                match serde_json::from_str(msg) {
                    Ok(parsed) => Ok(Some(parsed)),
                    Err(err) => Err(err.into()),
                }
            }
        };

        src.advance(len);
        self.remaining_msg_bytes = 0;

        result
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
    use serde_json::Value;

    use super::*;

    #[test]
    fn encode_and_decode() {
        let decoded = r#"{"jsonrpc":"2.0","method":"exit"}"#.to_string();
        let encoded = format!("Content-Length: {}\r\n\r\n{}", decoded.len(), decoded);

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
        let decoded = r#"{"jsonrpc":"2.0","method":"exit"}"#.to_string();
        let content_len = format!("Content-Length: {}", decoded.len());
        let content_type = "Content-Type: application/vscode-jsonrpc; charset=utf-8".to_string();
        let encoded = format!("{}\r\n{}\r\n\r\n{}", content_len, content_type, decoded);

        let mut codec = LanguageServerCodec::default();
        let mut buffer = BytesMut::from(encoded.as_str());
        let message = codec.decode(&mut buffer).unwrap();
        let decoded: Value = serde_json::from_str(&decoded).unwrap();
        assert_eq!(message, Some(decoded));
    }

    #[test]
    fn recovers_from_parse_error() {
        let decoded = r#"{"jsonrpc":"2.0","method":"exit"}"#.to_string();
        let encoded = format!("Content-Length: {}\r\n\r\n{}", decoded.len(), decoded);
        let mixed = format!("1234567890abcdefgh{}", encoded);

        let mut codec = LanguageServerCodec::default();
        let mut buffer = BytesMut::from(mixed.as_str());

        match codec.decode(&mut buffer) {
            Err(ParseError::MissingHeader) => {}
            other => panic!("expected `Err(ParseError::MissingHeader)`, got {:?}", other),
        }

        let message = codec.decode(&mut buffer).unwrap();
        let decoded: Value = serde_json::from_str(&decoded).unwrap();
        assert_eq!(message, Some(decoded));
    }
}
