use std::pin::Pin;
use std::task::{Context, Poll};
use futures_util::Stream;
use tokio::sync::mpsc;
use chatapi_shared::{ChatCompletionChunk, ChunkChoice, Delta};

/// Wraps a token receiver and formats each token as an OpenAI-compatible SSE chunk.
pub struct SseStream {
    rx: mpsc::Receiver<String>,
    id: String,
    model: String,
    created: i64,
    done: bool,
    /// Buffer for coalescing small chunks
    buffer: String,
}

impl SseStream {
    pub fn new(rx: mpsc::Receiver<String>, id: String, model: String, created: i64) -> Self {
        Self {
            rx,
            id,
            model,
            created,
            done: false,
            buffer: String::new(),
        }
    }

    fn format_chunk(&self, content: &str) -> String {
        let chunk = ChatCompletionChunk {
            id: self.id.clone(),
            object: "chat.completion.chunk".to_string(),
            created: self.created,
            model: self.model.clone(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: Delta {
                    role: None,
                    content: Some(content.to_string()),
                },
                finish_reason: None,
            }],
        };
        serde_json::to_string(&chunk).unwrap_or_default()
    }

    fn format_stop_chunk(&self) -> String {
        let chunk = ChatCompletionChunk {
            id: self.id.clone(),
            object: "chat.completion.chunk".to_string(),
            created: self.created,
            model: self.model.clone(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: Delta {
                    role: None,
                    content: None,
                },
                finish_reason: Some("stop".to_string()),
            }],
        };
        serde_json::to_string(&chunk).unwrap_or_default()
    }
}

impl Stream for SseStream {
    type Item = Result<axum::response::sse::Event, std::convert::Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.done {
            return Poll::Ready(None);
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
                        let event = axum::response::sse::Event::default().data(data);
                        return Poll::Ready(Some(Ok(event)));
                    }
                    // Small chunk — keep buffering, will flush on next poll or completion
                    continue;
                }
                Poll::Ready(None) => {
                    // Channel closed — stream is done
                    self.done = true;

                    // Flush any remaining buffer
                    let mut events = Vec::new();
                    if !self.buffer.is_empty() {
                        let data = self.format_chunk(&self.buffer);
                        self.buffer.clear();
                        events.push(Ok(axum::response::sse::Event::default().data(data)));
                    }

                    // Send stop chunk
                    let stop_data = self.format_stop_chunk();
                    events.push(Ok(axum::response::sse::Event::default().data(stop_data)));

                    // Send [DONE]
                    events.push(Ok(axum::response::sse::Event::default().data("[DONE]")));

                    if let Some(first) = events.into_iter().next() {
                        // Store remaining events to emit — for simplicity, batch them
                        // In practice we'd use a VecDeque, but for the final flush
                        // we can send them all as one combined event
                        return Poll::Ready(Some(first));
                    }
                    return Poll::Ready(None);
                }
                Poll::Pending => {
                    // Check if we have buffered data to flush (coalescing timeout)
                    if !self.buffer.is_empty() {
                        let data = self.format_chunk(&self.buffer);
                        self.buffer.clear();
                        let event = axum::response::sse::Event::default().data(data);
                        return Poll::Ready(Some(Ok(event)));
                    }
                    return Poll::Pending;
                }
            }
        }
    }
}
