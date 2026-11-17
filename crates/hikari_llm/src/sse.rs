/// Minimal incremental SSE decoder: collects `data:` lines per event, tolerates
/// `\r\n`, comments, other fields, and events split across byte chunks.
#[derive(Default)]
pub struct SseDecoder {
    buf: Vec<u8>,
    data: Vec<String>,
}

impl SseDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed raw bytes; returns the `data` payloads of all completed events.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<String> {
        self.buf.extend_from_slice(bytes);
        let mut events = Vec::new();
        while let Some(pos) = self.buf.iter().position(|b| *b == b'\n') {
            let mut line: Vec<u8> = self.buf.drain(..=pos).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            self.handle_line(&line, &mut events);
        }
        events
    }

    /// Flush a trailing event not terminated by a blank line at end of stream.
    pub fn finish(&mut self) -> Vec<String> {
        let mut events = Vec::new();
        if !self.buf.is_empty() {
            let line = std::mem::take(&mut self.buf);
            let line = line.strip_suffix(b"\r").unwrap_or(&line).to_vec();
            self.handle_line(&line, &mut events);
        }
        if !self.data.is_empty() {
            events.push(self.data.join("\n"));
            self.data.clear();
        }
        events
    }

    fn handle_line(&mut self, line: &[u8], events: &mut Vec<String>) {
        if line.is_empty() {
            if !self.data.is_empty() {
                events.push(self.data.join("\n"));
                self.data.clear();
            }
            return;
        }
        if line.starts_with(b"data:") {
            let rest = &line[5..];
            let rest = rest.strip_prefix(b" ").unwrap_or(rest);
            self.data.push(String::from_utf8_lossy(rest).into_owned());
        }
    }
}

#[cfg(test)]
#[path = "sse_tests.rs"]
mod tests;
