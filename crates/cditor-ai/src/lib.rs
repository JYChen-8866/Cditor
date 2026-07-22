mod mock;
#[cfg(feature = "openai")]
mod openai;
mod provider;

pub use mock::MockAiProvider;
#[cfg(feature = "openai")]
pub use openai::{OpenAiCompatibleConfig, OpenAiCompatibleProvider};
pub use provider::{
    AiCancellationToken, AiProvider, AiProviderError, AiProviderRequest, AiStreamEvent, AiTaskKind,
    DEFAULT_AI_STREAM_CAPACITY, bounded_ai_stream,
};
