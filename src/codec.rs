//! Encoder and decoder for Language Server Protocol messages.

use std::error::Error;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::io::{Error as IoError, Write};
use std::str::{self, Utf8Error};

use bytes::{BufMut, BytesMut};
use nom::bytes::streaming::tag;
use nom::character::streaming::digit1;
use nom::combinator::{map, map_res};
use nom::error::ErrorKind;
use nom::multi::length_data;
use nom::sequence::delimited;
use nom::{Err, IResult, Needed};
use tokio_codec::{Decoder, Encoder};

/// Errors that can occur when processing an LSP request.
#[derive(Debug)]
pub enum ParseError {
    /// Request lacks the required `Content-Length` header.
    MissingHeader,
    /// The length value in the `Content-Length` header is invalid.
    InvalidLength,
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

impl Encoder for LanguageServerCodec {
    type Item = String;
    type Error = ParseError;

    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
        if !item.is_empty() {
            dst.reserve(item.len() + 30);
            let mut writer = dst.writer();
            write!(writer, "Content-Length: {}\r\n\r\n{}", item.len(), item)?;
            writer.flush()?;
        }

        Ok(())
    }
}

impl Decoder for LanguageServerCodec {
    type Item = String;
    type Error = ParseError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if self.remaining_msg_bytes > src.len() {
            return Ok(None);
        }

        let string = str::from_utf8(src)?;
        let (message, len) = match parse_request(string) {
            Ok((remaining, message)) => (message.to_string(), src.len() - remaining.len()),
            Err(Err::Incomplete(Needed::Size(min))) => {
                self.remaining_msg_bytes = min;
                return Ok(None);
            }
            Err(Err::Incomplete(_)) => {
                return Ok(None);
            }
            Err(Err::Error((_, err))) | Err(Err::Failure((_, err))) => match err {
                ErrorKind::Digit | ErrorKind::MapRes => return Err(ParseError::InvalidLength),
                _ => return Err(ParseError::MissingHeader),
            },
        };

        src.advance(len);
        self.remaining_msg_bytes = 0;

        Ok(Some(message))
    }
}

fn parse_request(input: &str) -> IResult<&str, String> {
    let content_len = delimited(tag("Content-Length: "), digit1, tag("\r\n\r\n"));
    let header = map_res(content_len, |s: &str| s.parse::<usize>());
    let message = length_data(header);
    map(message, |msg| msg.to_string())(input)
}

#[cfg(test)]
mod tests {
    use bytes::BytesMut;

    use super::*;

    #[test]
    fn round_trip() {
        let decoded = r#"{"jsonrpc":"2.0","method":"exit"}"#.to_string();
        let encoded = format!("Content-Length: {}\r\n\r\n{}", decoded.len(), decoded);

        let mut codec = LanguageServerCodec::default();
        let mut buffer = BytesMut::new();
        codec.encode(decoded.clone(), &mut buffer).unwrap();
        assert_eq!(buffer, BytesMut::from(encoded.clone()));

        let mut buffer = BytesMut::from(encoded);
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
}
