use std::time::Duration;

use crate::provider::{
    AiCancellationToken, AiProvider, AiProviderError, AiProviderRequest, AiStreamEvent, AiTaskKind,
    send_stream_event,
};

#[derive(Debug, Clone)]
pub struct MockAiProvider {
    chunk_bytes: usize,
    chunk_delay: Duration,
}

impl Default for MockAiProvider {
    fn default() -> Self {
        Self {
            chunk_bytes: 18,
            chunk_delay: Duration::from_millis(24),
        }
    }
}

impl MockAiProvider {
    pub fn instant() -> Self {
        Self {
            chunk_bytes: 18,
            chunk_delay: Duration::ZERO,
        }
    }

    pub fn with_chunking(chunk_bytes: usize, chunk_delay: Duration) -> Self {
        Self {
            chunk_bytes: chunk_bytes.max(1),
            chunk_delay,
        }
    }

    fn response_for(&self, request: &AiProviderRequest) -> String {
        match request.task {
            AiTaskKind::InlineCompletion => {
                " Continue the thought with a clear, concrete next sentence.".to_owned()
            }
            AiTaskKind::RewriteSelection | AiTaskKind::RewriteBlocks => {
                let source = request.selected_text.trim();
                let instruction = request.instruction.trim();
                if source.is_empty() {
                    format!("AI draft for: {instruction}")
                } else if contains_any(instruction, &["short", "shorter", "缩短", "精简"]) {
                    shorten(source, 120)
                } else if contains_any(instruction, &["long", "longer", "扩写", "详细"]) {
                    format!(
                        "{source}\n\nThis version adds the supporting detail needed to make the point easier to understand."
                    )
                } else if contains_any(instruction, &["translate", "翻译"]) {
                    format!("Translated draft: {source}")
                } else {
                    normalize_whitespace(source)
                }
            }
        }
    }
}

impl AiProvider for MockAiProvider {
    fn id(&self) -> &str {
        "mock"
    }

    fn stream(
        &self,
        request: AiProviderRequest,
        sender: async_channel::Sender<AiStreamEvent>,
        cancellation: AiCancellationToken,
    ) -> Result<(), AiProviderError> {
        let response = self.response_for(&request);
        for chunk in utf8_chunks(&response, self.chunk_bytes) {
            send_stream_event(
                &sender,
                AiStreamEvent::Delta {
                    request_id: request.request_id,
                    text: chunk.to_owned(),
                },
                &cancellation,
            )?;
            if !self.chunk_delay.is_zero() {
                std::thread::sleep(self.chunk_delay);
            }
        }
        send_stream_event(
            &sender,
            AiStreamEvent::Done {
                request_id: request.request_id,
            },
            &cancellation,
        )
    }
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    let value = value.to_lowercase();
    needles.iter().any(|needle| value.contains(needle))
}

fn shorten(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }
    let mut end = max_bytes.min(value.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", value[..end].trim_end())
}

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn utf8_chunks(value: &str, max_bytes: usize) -> Vec<&str> {
    if value.is_empty() {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < value.len() {
        let mut end = (start + max_bytes).min(value.len());
        while end > start && !value.is_char_boundary(end) {
            end -= 1;
        }
        if end == start {
            end = value[start..]
                .char_indices()
                .nth(1)
                .map(|(offset, _)| start + offset)
                .unwrap_or(value.len());
        }
        chunks.push(&value[start..end]);
        start = end;
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AiProviderRequest, bounded_ai_stream};

    fn request(task: AiTaskKind, text: &str, instruction: &str) -> AiProviderRequest {
        AiProviderRequest {
            request_id: 3,
            task,
            instruction: instruction.to_owned(),
            selected_text: text.to_owned(),
            prefix: String::new(),
            suffix: String::new(),
        }
    }

    #[test]
    fn mock_provider_streams_utf8_safe_chunks_and_done() {
        let provider = MockAiProvider::with_chunking(2, Duration::ZERO);
        let (sender, receiver) = bounded_ai_stream(64);
        provider
            .stream(
                request(AiTaskKind::RewriteSelection, "你好  世界", "改善写作"),
                sender,
                AiCancellationToken::default(),
            )
            .unwrap();
        let mut events = Vec::new();
        while let Ok(event) = receiver.try_recv() {
            events.push(event);
        }
        assert!(matches!(events.last(), Some(AiStreamEvent::Done { .. })));
        let result = events
            .iter()
            .filter_map(|event| match event {
                AiStreamEvent::Delta { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<String>();
        assert_eq!(result, "你好 世界");
    }

    #[test]
    fn cancelled_mock_provider_does_not_emit_content() {
        let provider = MockAiProvider::instant();
        let token = AiCancellationToken::default();
        token.cancel();
        let (sender, receiver) = bounded_ai_stream(8);
        assert_eq!(
            provider.stream(
                request(AiTaskKind::InlineCompletion, "", "continue"),
                sender,
                token,
            ),
            Err(AiProviderError::Cancelled)
        );
        assert!(receiver.is_empty());
    }
}
