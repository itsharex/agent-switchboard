//! Bounded decoding for compressed gateway bodies.
//!
//! Codex Desktop may send its loopback Responses requests with
//! `Content-Encoding: zstd`. The gateway must decode before JSON parsing, then
//! deliberately sends a fresh uncompressed JSON body to the upstream.

use reqwest::header::{HeaderMap, CONTENT_ENCODING};
use std::io::{self, BufRead, BufReader, Cursor, Read};
use tiny_http::Header;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum DecodeError {
    Unsupported(String),
    Invalid(String),
    TooLarge,
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(coding) => write!(formatter, "不支持 Content-Encoding: {coding}"),
            Self::Invalid(error) => write!(formatter, "Content-Encoding 数据无效: {error}"),
            Self::TooLarge => formatter.write_str("解码后的内容超过网关限制"),
        }
    }
}

impl std::error::Error for DecodeError {}

pub(super) fn decode_request_body(
    headers: &[Header],
    body: &[u8],
    limit: usize,
) -> Result<Vec<u8>, DecodeError> {
    decode_body(content_codings(headers), body, limit)
}

/// Decodes a complete upstream response before JSON or error parsing. The
/// caller owns the response-size budget; this function applies it again after
/// every content-coding layer so a compressed response cannot expand past the
/// gateway's operation limit.
pub(super) fn decode_response_body(
    headers: &HeaderMap,
    body: &[u8],
    limit: usize,
) -> Result<Vec<u8>, DecodeError> {
    let values = headers
        .get_all(CONTENT_ENCODING)
        .iter()
        .filter_map(|value| value.to_str().ok());
    decode_body(content_codings_from_values(values), body, limit)
}

/// Wraps an upstream streaming body in decoders without buffering the whole
/// response. Every encoded layer has its own decoded-size budget, so an
/// intermediate representation cannot expand without bound before a later
/// decoder consumes it. Some OpenAI-compatible gateways emit raw deflate
/// despite the HTTP zlib convention, so select its wrapper from the first two
/// bytes.
pub(super) fn decode_response_stream<R>(
    headers: &HeaderMap,
    body: R,
    limit: usize,
) -> Result<Box<dyn Read>, DecodeError>
where
    R: Read + 'static,
{
    let values = headers
        .get_all(CONTENT_ENCODING)
        .iter()
        .filter_map(|value| value.to_str().ok());
    let codings = content_codings_from_values(values);
    if let Some(coding) = codings.iter().find(|coding| !supported(coding)) {
        return Err(DecodeError::Unsupported(coding.to_string()));
    }
    let mut reader: Box<dyn Read> = Box::new(LimitedReader::new(body, limit));
    for coding in codings.iter().rev() {
        let decoded: Box<dyn Read> = match coding.as_str() {
            "gzip" | "x-gzip" => Box::new(flate2::read::GzDecoder::new(reader)),
            "deflate" => decode_deflate_stream(reader)?,
            "br" => Box::new(brotli::Decompressor::new(reader, 4096)),
            "zstd" | "zst" => Box::new(
                zstd::stream::read::Decoder::new(reader)
                    .map_err(|error| DecodeError::Invalid(error.to_string()))?,
            ),
            other => return Err(DecodeError::Unsupported(other.to_string())),
        };
        reader = Box::new(DecodedLayer::new(LimitedReader::new(decoded, limit)));
    }
    Ok(reader)
}

#[derive(Debug)]
enum StreamDecodeFailure {
    TooLarge,
    Invalid(String),
}

impl std::fmt::Display for StreamDecodeFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge => formatter.write_str("解码后的内容超过网关限制"),
            Self::Invalid(error) => write!(formatter, "Content-Encoding 数据无效: {error}"),
        }
    }
}

impl std::error::Error for StreamDecodeFailure {}

/// Recovers a decoding failure that occurred after a streaming response has
/// begun. Transport errors deliberately remain ordinary I/O errors so callers
/// can continue to report them as network or timeout failures.
pub(super) fn stream_decode_error(error: &io::Error) -> Option<DecodeError> {
    let failure = error.get_ref()?.downcast_ref::<StreamDecodeFailure>()?;
    Some(match failure {
        StreamDecodeFailure::TooLarge => DecodeError::TooLarge,
        StreamDecodeFailure::Invalid(error) => DecodeError::Invalid(error.clone()),
    })
}

fn stream_decode_failure(failure: StreamDecodeFailure) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, failure)
}

fn is_stream_decode_failure(error: &io::Error) -> bool {
    error
        .get_ref()
        .is_some_and(|source| source.is::<StreamDecodeFailure>())
}

/// Bounds one encoded representation. It probes the source after exactly the
/// limit so an otherwise valid stream that is one byte too large is rejected.
struct LimitedReader<R> {
    source: R,
    remaining: u64,
}

impl<R: Read> LimitedReader<R> {
    fn new(source: R, limit: usize) -> Self {
        Self {
            source,
            remaining: limit as u64,
        }
    }
}

impl<R: Read> Read for LimitedReader<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        if self.remaining == 0 {
            let mut probe = [0_u8; 1];
            return match self.source.read(&mut probe) {
                Ok(0) => Ok(0),
                Ok(_) => Err(stream_decode_failure(StreamDecodeFailure::TooLarge)),
                Err(error) => Err(error),
            };
        }
        let capacity = output.len().min(self.remaining as usize);
        let count = self.source.read(&mut output[..capacity])?;
        self.remaining -= count as u64;
        Ok(count)
    }
}

/// Converts errors emitted by a content decoder into a typed decoding failure
/// while allowing cancellation and transport timeouts to retain their origin.
struct DecodedLayer<R> {
    source: R,
}

impl<R> DecodedLayer<R> {
    fn new(source: R) -> Self {
        Self { source }
    }
}

impl<R: Read> Read for DecodedLayer<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.source.read(output).map_err(|error| {
            if is_stream_decode_failure(&error)
                || matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut
                        | io::ErrorKind::ConnectionAborted
                        | io::ErrorKind::Interrupted
                        | io::ErrorKind::WouldBlock
                )
            {
                error
            } else {
                stream_decode_failure(StreamDecodeFailure::Invalid(error.to_string()))
            }
        })
    }
}

fn decode_deflate_stream(reader: Box<dyn Read>) -> Result<Box<dyn Read>, DecodeError> {
    let mut buffered = BufReader::new(reader);
    let header = buffered
        .fill_buf()
        .map_err(|error| DecodeError::Invalid(error.to_string()))?;
    if is_zlib_header(header) {
        Ok(Box::new(flate2::read::ZlibDecoder::new(buffered)))
    } else {
        Ok(Box::new(flate2::read::DeflateDecoder::new(buffered)))
    }
}

fn is_zlib_header(header: &[u8]) -> bool {
    header.len() >= 2
        && header[0] & 0x0f == 8
        && ((u16::from(header[0]) << 8) | u16::from(header[1])) % 31 == 0
}

fn decode_body(codings: Vec<String>, body: &[u8], limit: usize) -> Result<Vec<u8>, DecodeError> {
    if codings.is_empty() {
        return Ok(body.to_vec());
    }
    if let Some(coding) = codings.iter().find(|coding| !supported(coding)) {
        return Err(DecodeError::Unsupported(coding.to_string()));
    }
    let mut decoded = body.to_vec();
    for coding in codings.iter().rev() {
        decoded = decode_one(coding, &decoded, limit)?;
    }
    Ok(decoded)
}

fn content_codings(headers: &[Header]) -> Vec<String> {
    content_codings_from_values(
        headers
            .iter()
            .filter(|header| header.field.equiv("Content-Encoding"))
            .map(|header| header.value.as_str()),
    )
}

fn content_codings_from_values<'a>(values: impl Iterator<Item = &'a str>) -> Vec<String> {
    values
        .flat_map(|value| value.split(','))
        .map(|coding| coding.trim().to_ascii_lowercase())
        .filter(|coding| !coding.is_empty() && coding != "identity")
        .collect()
}

fn supported(coding: &str) -> bool {
    matches!(
        coding,
        "gzip" | "x-gzip" | "deflate" | "br" | "zstd" | "zst"
    )
}

fn decode_one(coding: &str, body: &[u8], limit: usize) -> Result<Vec<u8>, DecodeError> {
    match coding {
        "gzip" | "x-gzip" => read_limited(flate2::read::GzDecoder::new(body), limit),
        "deflate" => match read_limited(flate2::read::ZlibDecoder::new(body), limit) {
            Ok(output) => Ok(output),
            Err(DecodeError::TooLarge) => Err(DecodeError::TooLarge),
            Err(DecodeError::Invalid(_)) => {
                read_limited(flate2::read::DeflateDecoder::new(body), limit)
            }
            Err(error) => Err(error),
        },
        "br" => read_limited(brotli::Decompressor::new(Cursor::new(body), 4096), limit),
        "zstd" | "zst" => {
            let reader = zstd::stream::read::Decoder::new(Cursor::new(body))
                .map_err(|error| DecodeError::Invalid(error.to_string()))?;
            read_limited(reader, limit)
        }
        other => Err(DecodeError::Unsupported(other.to_string())),
    }
}

fn read_limited(reader: impl Read, limit: usize) -> Result<Vec<u8>, DecodeError> {
    let mut reader = reader.take(limit.saturating_add(1) as u64);
    let mut output = Vec::new();
    reader
        .read_to_end(&mut output)
        .map_err(|error| DecodeError::Invalid(error.to_string()))?;
    if output.len() > limit {
        return Err(DecodeError::TooLarge);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::thread;
    use tiny_http::{Response, Server};

    fn headers(value: &str) -> Vec<Header> {
        vec![Header::from_bytes("Content-Encoding", value).unwrap()]
    }

    fn response_headers(values: &[&str]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for value in values {
            headers.append(CONTENT_ENCODING, value.parse().unwrap());
        }
        headers
    }

    fn gzip(payload: &[u8]) -> Vec<u8> {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(payload).unwrap();
        encoder.finish().unwrap()
    }

    fn zlib(payload: &[u8]) -> Vec<u8> {
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(payload).unwrap();
        encoder.finish().unwrap()
    }

    fn raw_deflate(payload: &[u8]) -> Vec<u8> {
        let mut encoder =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(payload).unwrap();
        encoder.finish().unwrap()
    }

    fn brotli(payload: &[u8]) -> Vec<u8> {
        let mut compressed = Vec::new();
        {
            let mut encoder = brotli::CompressorWriter::new(&mut compressed, 4096, 5, 22);
            encoder.write_all(payload).unwrap();
        }
        compressed
    }

    fn zstd(payload: &[u8]) -> Vec<u8> {
        zstd::stream::encode_all(Cursor::new(payload), 0).unwrap()
    }

    #[test]
    fn decodes_codex_zstd_request() {
        let payload = br#"{"model":"sandbox","input":"hello"}"#;
        let compressed = zstd(&payload[..]);
        assert_eq!(
            decode_request_body(&headers("zstd"), &compressed, 1024).unwrap(),
            payload
        );
    }

    #[test]
    fn decodes_the_full_request_coding_matrix() {
        let payload = br#"{"model":"sandbox","input":"hello"}"#;
        for (coding, encoded) in [
            ("identity", payload.to_vec()),
            ("gzip", gzip(payload)),
            ("x-gzip", gzip(payload)),
            ("zstd", zstd(payload)),
            ("zst", zstd(payload)),
            ("br", brotli(payload)),
            ("deflate", zlib(payload)),
            ("deflate", raw_deflate(payload)),
        ] {
            assert_eq!(
                decode_request_body(&headers(coding), &encoded, 1024).unwrap(),
                payload,
                "{coding} request must decode"
            );
        }
    }

    #[test]
    fn decodes_stacked_and_repeated_codings_in_reverse_order() {
        let payload = br#"{"ok":true}"#;
        let compressed = zstd(&gzip(payload));
        let repeated_headers = vec![
            Header::from_bytes("Content-Encoding", "gzip").unwrap(),
            Header::from_bytes("Content-Encoding", "zstd").unwrap(),
        ];
        assert_eq!(
            decode_request_body(&repeated_headers, &compressed, 1024).unwrap(),
            payload
        );
        let mut decoded = decode_response_stream(
            &response_headers(&["gzip", "zstd"]),
            Cursor::new(compressed),
            1024,
        )
        .unwrap();
        let mut output = Vec::new();
        decoded.read_to_end(&mut output).unwrap();
        assert_eq!(output, payload);
    }

    #[test]
    fn refuses_unknown_and_oversized_encoded_bodies() {
        assert_eq!(
            decode_request_body(&headers("snappy"), b"bytes", 1024),
            Err(DecodeError::Unsupported("snappy".to_string()))
        );
        let compressed = zstd(&vec![0; 2048]);
        assert_eq!(
            decode_request_body(&headers("zstd"), &compressed, 1024),
            Err(DecodeError::TooLarge)
        );
    }

    #[test]
    fn decodes_the_full_response_stream_coding_matrix() {
        let payload = b"data: {\"type\":\"response.completed\"}\n\n";
        for (coding, encoded) in [
            ("identity", payload.to_vec()),
            ("gzip", gzip(payload)),
            ("x-gzip", gzip(payload)),
            ("zstd", zstd(payload)),
            ("zst", zstd(payload)),
            ("br", brotli(payload)),
            ("deflate", zlib(payload)),
            ("deflate", raw_deflate(payload)),
        ] {
            let headers = response_headers(&[coding]);
            assert_eq!(
                decode_response_body(&headers, &encoded, 1024).unwrap(),
                payload,
                "{coding} response body must decode"
            );
            let mut decoded = decode_response_stream(&headers, Cursor::new(encoded), 1024).unwrap();
            let mut output = Vec::new();
            decoded.read_to_end(&mut output).unwrap();
            assert_eq!(output, payload, "{coding} response stream must decode");
        }
    }

    #[test]
    fn streaming_decoder_reports_corrupt_and_oversized_content() {
        let headers = response_headers(&["gzip"]);
        let mut corrupt =
            decode_response_stream(&headers, Cursor::new(b"not a gzip stream"), 1024).unwrap();
        let error = corrupt.read(&mut [0_u8; 32]).unwrap_err();
        assert!(matches!(
            stream_decode_error(&error),
            Some(DecodeError::Invalid(_))
        ));

        let payload = vec![0_u8; 2048];
        let mut oversized =
            decode_response_stream(&headers, Cursor::new(gzip(&payload)), 1024).unwrap();
        let error = oversized.read_to_end(&mut Vec::new()).unwrap_err();
        assert_eq!(stream_decode_error(&error), Some(DecodeError::TooLarge));
    }

    #[test]
    fn decodes_raw_deflate_upstream_stream() {
        let payload = b"data: {\"type\":\"response.completed\"}\n\n";
        let mut encoder =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(payload).unwrap();
        let compressed = encoder.finish().unwrap();
        let upstream = Server::http(("127.0.0.1", 0)).unwrap();
        let address = upstream.server_addr().to_string();
        let worker = thread::spawn(move || {
            let request = upstream.recv().unwrap();
            request
                .respond(
                    Response::from_data(compressed)
                        .with_header(Header::from_bytes("Content-Encoding", "deflate").unwrap()),
                )
                .unwrap();
        });
        let response = reqwest::blocking::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .get(format!("http://{address}"))
            .send()
            .unwrap();
        let headers = response.headers().clone();
        let mut decoded = decode_response_stream(&headers, response, 1024).unwrap();
        let mut output = Vec::new();
        decoded.read_to_end(&mut output).unwrap();
        assert_eq!(output, payload);
        worker.join().unwrap();
    }
}
