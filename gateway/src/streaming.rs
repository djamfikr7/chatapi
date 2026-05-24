use std::pin::Pin;
use std::task::{Context, Poll};
use futures_util::Stream;
use tokio::sync::mpsc;
use chatapi_shared::ChatCompletionChunk;

/// Wraps a token receiver and formats each token as an OpenAI-compatible SSE chunk.
pub struct SseStream {
    rx: mpsc::Receiver<String>,
    id: String,
    model: String,
    /// Buffer for coalescing small chunks
    buffer: String,
    /// Pending events to emit on next poll
    pending: Vec<String>,
    /// Whether the stream has finished (stop + [DONE] sent)
    done: bool,
}

impl SseStream {
    pub fn new(rx: mpsc::Receiver<String>, id: String, model: String) -> Self {
        Self {
            rx,
            id,
            model,
            buffer: String::new(),
            pending: Vec::new(),
            done: false,
        }
    }

    fn format_chunk(&self, content: &str) -> String {
        let chunk = ChatCompletionChunk::new_delta(&self.model, &self.id, content);
        serde_json::to_string(&chunk).unwrap_or_default()
    }

    fn format_stop_chunk(&self) -> String {
        let chunk = ChatCompletionChunk::new_finish(&self.model, &self.id);
        serde_json::to_string(&chunk).unwrap_or_default()
    }

    fn emit_event(data: String) -> Result<axum::response::sse::Event, std::convert::Infallible> {
        Ok(axum::response::sse::Event::default().data(data))
    }
}

impl Stream for SseStream {
    type Item = Result<axum::response::sse::Event, std::convert::Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // If stream is done, return None
        if self.done {
            return Poll::Ready(None);
        }

        // Drain pending events first
        if !self.pending.is_empty() {
            let data = self.pending.remove(0);
            if self.pending.is_empty() {
                self.done = true;
            }
            return Poll::Ready(Some(Self::emit_event(data)));
        }

        loop {
            match self.rx.poll_recv(cx) {
                Poll::Ready(Some(token)) => {
                    // Adaptive chunk coalescing: buffer small tokens, flush large ones
                    self.buffer.push_str(&token);

                    if self.buffer.len() >= 256 {
                        // Flush immediately for large chunks
                        let data = self.format_chunk(&self.buffer);
                        self.buffer.clear();
                        return Poll::Ready(Some(Self::emit_event(data)));
                    }
                    // Small chunk — keep buffering, will flush on next poll or completion
                    continue;
                }
                Poll::Ready(None) => {
                    // Channel closed — stream is done
                    let mut events = Vec::new();

                    // Flush any remaining buffer
                    if !self.buffer.is_empty() {
                        events.push(self.format_chunk(&self.buffer));
                        self.buffer.clear();
                    }

                    // Send stop chunk
                    events.push(self.format_stop_chunk());

                    // Send [DONE]
                    events.push("[DONE]".to_string());

                    if events.is_empty() {
                        return Poll::Ready(None);
                    }

                    // Return first, stash the rest
                    let first = events.remove(0);
                    self.pending = events;
                    return Poll::Ready(Some(Self::emit_event(first)));
                }
                Poll::Pending => {
                    // Check if we have buffered data to flush (coalescing timeout)
                    if !self.buffer.is_empty() {
                        let data = self.format_chunk(&self.buffer);
                        self.buffer.clear();
                        return Poll::Ready(Some(Self::emit_event(data)));
                    }
                    return Poll::Pending;
                }
            }
        }
    }
}
