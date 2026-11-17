#![allow(clippy::unwrap_used)]

use crate::sse::SseDecoder;

#[test]
fn single_event() {
    let mut dec = SseDecoder::new();
    let events = dec.feed(b"data: hello\n\n");
    assert_eq!(events, ["hello"]);
}

#[test]
fn event_split_across_chunks() {
    let mut dec = SseDecoder::new();
    assert!(dec.feed(b"data: hel").is_empty());
    assert!(dec.feed(b"lo\n").is_empty());
    assert_eq!(dec.feed(b"\n"), ["hello"]);
}

#[test]
fn crlf_line_endings() {
    let mut dec = SseDecoder::new();
    assert_eq!(dec.feed(b"data: hi\r\n\r\n"), ["hi"]);
}

#[test]
fn multi_line_data_joined() {
    let mut dec = SseDecoder::new();
    assert_eq!(dec.feed(b"data: a\ndata: b\n\n"), ["a\nb"]);
}

#[test]
fn comments_and_other_fields_ignored() {
    let mut dec = SseDecoder::new();
    assert_eq!(
        dec.feed(b": comment\nevent: msg\nid: 1\ndata: x\n\n"),
        ["x"]
    );
}

#[test]
fn multiple_events_in_one_chunk() {
    let mut dec = SseDecoder::new();
    assert_eq!(dec.feed(b"data: a\n\ndata: b\n\n"), ["a", "b"]);
}

#[test]
fn finish_flushes_trailing_event() {
    let mut dec = SseDecoder::new();
    assert!(dec.feed(b"data: tail\n").is_empty());
    assert_eq!(dec.finish(), ["tail"]);
}
