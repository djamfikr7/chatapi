use std::pin::Pin;
use std::task::{Context, Poll};
use futures_util::Stream;
use tokio::sync::mpsc;
use chatapi_shared::{
    ChatCompletionChunk,
    tool_parser::{contains_tool_call_pattern, parse_tool_calls_from_text},
};

/// Wraps a token receiver and formats each token as an OpenAI-compatible SSE chunk.
///
/// Text tokens are buffered and streamed in chunks. When the source closes,
/// the accumulated text is checked for tool call patterns — if found, the
/// remaining output switches to tool_calls format.
pub struct SseStream {
    rx: mpsc::Receiver<String>,
    id: String,
    model: String,
    /// Text buffer for the current chunk being built.
    buffer: String,
    /// All accumulated text (for final tool call detection).
    accumulated: String,
    /// Pending SSE event data strings to emit.
    pending: Vec<String>,
    /// Stream is finished.
    done: bool,
    /// Whether we've already flushed the final events.
    final_flushed: bool,
}

impl SseStream {
    pub fn new(rx: mpsc::Receiver<String>, id: String, model: String) -> Self {
        Self {
            rx,
            id,
            model,
            buffer: String::new(),
            accumulated: String::new(),
            pending: Vec::new(),
            done: false,
            final_flushed: false,
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

    fn format_tool_calls_stop_chunk(&self) -> String {
        let chunk = ChatCompletionChunk::new_tool_calls_finish(&self.model, &self.id);
        serde_json::to_string(&chunk).unwrap_or_default()
    }

    fn format_tool_call_chunk(
        &self,
        index: u32,
        call_id: Option<&str>,
        call_type: Option<&str>,
        fn_name: Option<&str>,
        fn_args: Option<&str>,
    ) -> String {
        let chunk = ChatCompletionChunk::new_tool_call_delta(
            &self.model,
            &self.id,
            index,
            call_id,
            call_type,
            fn_name,
            fn_args,
        );
        serde_json::to_string(&chunk).unwrap_or_default()
    }

    fn make_event(data: String) -> axum::response::sse::Event {
        axum::response::sse::Event::default().data(data)
    }

    /// Generate final events based on accumulated text.
    fn flush_final(&mut self) {
        if self.final_flushed {
            return;
        }
        self.final_flushed = true;

        // Flush any remaining buffer
        if !self.buffer.is_empty() {
            self.accumulated.push_str(&self.buffer);
            self.pending.push(self.format_chunk(&self.buffer));
            self.buffer.clear();
        }

        // Check for tool calls in the full accumulated text
        if contains_tool_call_pattern(&self.accumulated) {
            let result = parse_tool_calls_from_text(&self.accumulated);
            if result.has_tool_calls {
                // Clear pending text chunks — we'll replace with tool call events
                self.pending.clear();

                // Emit prefix text as regular content
                if !result.prefix_text.is_empty() {
                    self.pending.push(self.format_chunk(&result.prefix_text));
                }

                // Emit tool call chunks
                for (i, tc) in result.tool_calls.iter().enumerate() {
                    // First: id + type + function name
                    self.pending.push(self.format_tool_call_chunk(
                        i as u32,
                        Some(&tc.id),
                        Some(&tc.call_type),
                        Some(&tc.function.name),
                        None,
                    ));
                    // Second: arguments
                    self.pending.push(self.format_tool_call_chunk(
                        i as u32,
                        None,
                        None,
                        None,
                        Some(&tc.function.arguments),
                    ));
                }

                // Emit suffix text if any
                if !result.suffix_text.is_empty() {
                    self.pending.push(self.format_chunk(&result.suffix_text));
                }

                // finish_reason = "tool_calls"
                self.pending.push(self.format_tool_calls_stop_chunk());
                self.pending.push("data: [DONE]".to_string());
                return;
            }
        }

        // Regular text response — finish_reason = "stop"
        self.pending.push(self.format_stop_chunk());
        self.pending.push("data: [DONE]".to_string());
    }
}

impl Stream for SseStream {
    type Item = Result<axum::response::sse::Event, std::convert::Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.done {
            return Poll::Ready(None);
        }

        // Drain pending events first
        if !self.pending.is_empty() {
            let data = self.pending.remove(0);
            if self.pending.is_empty() {
                self.done = true;
            }
            return Poll::Ready(Some(Ok(Self::make_event(data))));
        }

        // Try to receive more tokens
        match self.rx.poll_recv(cx) {
            Poll::Ready(Some(token)) => {
                self.accumulated.push_str(&token);
                self.buffer.push_str(&token);

                // Flush buffer when it's large enough
                if self.buffer.len() >= 64 {
                    let data = self.format_chunk(&self.buffer);
                    self.buffer.clear();
                    return Poll::Ready(Some(Ok(Self::make_event(data))));
                }

                // Wake again to collect more
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Poll::Ready(None) => {
                // Channel closed — flush final events
                self.flush_final();
                if !self.pending.is_empty() {
                    let data = self.pending.remove(0);
                    if self.pending.is_empty() {
                        self.done = true;
                    }
                    return Poll::Ready(Some(Ok(Self::make_event(data))));
                }
                self.done = true;
                Poll::Ready(None)
            }
            Poll::Pending => {
                // Flush buffer on pending if it has content
                if !self.buffer.is_empty() {
                    let data = self.format_chunk(&self.buffer);
                    self.buffer.clear();
                    return Poll::Ready(Some(Ok(Self::make_event(data))));
                }
                Poll::Pending
            }
        }
    }
}
