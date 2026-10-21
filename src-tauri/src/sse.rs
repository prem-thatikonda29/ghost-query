//! Incremental Server-Sent Events line parser.
//!
//! Network chunks do not respect line boundaries (or UTF-8 boundaries), so
//! bytes are buffered until a full line is available. Only `data:` fields are
//! returned; comments (`: ...`), other fields and blank lines are ignored.

#[derive(Default)]
pub struct SseParser {
    buf: Vec<u8>,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed raw bytes; returns the payload of every `data:` line completed so far.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.buf.extend_from_slice(bytes);

        let mut events = Vec::new();
        let mut start = 0;
        while let Some(offset) = self.buf[start..].iter().position(|&b| b == b'\n') {
            let end = start + offset;
            let mut line = &self.buf[start..end];
            if line.last() == Some(&b'\r') {
                line = &line[..line.len() - 1];
            }
            if let Some(data) = line.strip_prefix(b"data:") {
                let data = data.strip_prefix(b" ").unwrap_or(data);
                events.push(String::from_utf8_lossy(data).into_owned());
            }
            start = end + 1;
        }
        self.buf.drain(..start);
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_complete_lines() {
        let mut p = SseParser::new();
        let out = p.push(b"data: {\"content\":\"hi\"}\n\ndata: [DONE]\n\n");
        assert_eq!(out, vec!["{\"content\":\"hi\"}", "[DONE]"]);
    }

    #[test]
    fn reassembles_line_split_across_chunks() {
        let mut p = SseParser::new();
        assert!(p.push(b"data: {\"cont").is_empty());
        assert!(p.push(b"ent\":\"hello\"}").is_empty());
        assert_eq!(p.push(b"\n\n"), vec!["{\"content\":\"hello\"}"]);
    }

    #[test]
    fn reassembles_multibyte_char_split_across_chunks() {
        let bytes = "data: héllo\n".as_bytes();
        let split = bytes.iter().position(|&b| b == 0xC3).unwrap() + 1;
        let mut p = SseParser::new();
        assert!(p.push(&bytes[..split]).is_empty());
        assert_eq!(p.push(&bytes[split..]), vec!["héllo"]);
    }

    #[test]
    fn ignores_comments_other_fields_and_blank_lines() {
        let mut p = SseParser::new();
        let out = p.push(b": OPENROUTER PROCESSING\n\nevent: ping\nid: 7\ndata: ok\n\n");
        assert_eq!(out, vec!["ok"]);
    }

    #[test]
    fn handles_crlf_and_missing_space_after_colon() {
        let mut p = SseParser::new();
        let out = p.push(b"data:one\r\ndata: two\r\n");
        assert_eq!(out, vec!["one", "two"]);
    }

    #[test]
    fn keeps_incomplete_trailing_line_buffered() {
        let mut p = SseParser::new();
        assert_eq!(p.push(b"data: a\ndata: b"), vec!["a"]);
        assert_eq!(p.push(b"\n"), vec!["b"]);
    }
}
