use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{Value, json};

use crate::provider::{
    AiCancellationToken, AiProvider, AiProviderError, AiProviderRequest, AiStreamEvent, AiTaskKind,
    send_stream_event,
};

#[derive(Clone)]
pub struct OpenAiCompatibleConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct AiFileConfig {
    base_url: Option<String>,
    model: Option<String>,
}

impl std::fmt::Debug for OpenAiCompatibleConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenAiCompatibleConfig")
            .field("api_key", &"[redacted]")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .finish()
    }
}

impl OpenAiCompatibleConfig {
    pub fn from_env() -> Result<Self, AiProviderError> {
        let process_api_key =
            first_non_empty_env(&["CDITOR_AI_API_KEY", "OPENAI_AUTH_TOKEN", "OPENAI_API_KEY"]);
        let process_base_url = first_non_empty_env(&["CDITOR_AI_BASE_URL", "OPENAI_BASE_URL"]);
        let process_model = first_non_empty_env(&["CDITOR_AI_MODEL", "OPENAI_MODEL"]);
        let process_config_path = first_non_empty_env(&["CDITOR_AI_CONFIG"]).map(PathBuf::from);

        // Load the nearest project .env without overriding values supplied by the process.
        // This keeps deployment and command-line environment variables authoritative.
        let dotenv_path = dotenvy::dotenv().ok();
        let config_path = process_config_path
            .or_else(|| first_non_empty_env(&["CDITOR_AI_CONFIG"]).map(PathBuf::from));
        let file_config = load_file_config(config_path.as_deref(), dotenv_path.as_deref())?;
        let api_key = process_api_key
            .or_else(|| {
                first_non_empty_env(&["CDITOR_AI_API_KEY", "OPENAI_AUTH_TOKEN", "OPENAI_API_KEY"])
            })
            .ok_or_else(|| {
                AiProviderError::InvalidConfiguration(
                    "CDITOR_AI_API_KEY, OPENAI_AUTH_TOKEN, or OPENAI_API_KEY is not configured"
                        .to_owned(),
                )
            })?;
        let base_url = process_base_url
            .or_else(|| first_non_empty_env(&["CDITOR_AI_BASE_URL", "OPENAI_BASE_URL"]))
            .or(file_config.base_url)
            .unwrap_or_else(|| "https://api.openai.com/v1".to_owned());
        let model = process_model
            .or_else(|| first_non_empty_env(&["CDITOR_AI_MODEL", "OPENAI_MODEL"]))
            .or(file_config.model)
            .unwrap_or_else(|| default_model_for_base_url(&base_url).to_owned());
        Self::new(api_key, base_url, model)
    }

    pub fn new(
        api_key: impl Into<String>,
        base_url: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self, AiProviderError> {
        let config = Self {
            api_key: api_key.into().trim().to_owned(),
            base_url: base_url.into().trim().trim_end_matches('/').to_owned(),
            model: model.into().trim().to_owned(),
        };
        if config.api_key.is_empty() || config.base_url.is_empty() || config.model.is_empty() {
            return Err(AiProviderError::InvalidConfiguration(
                "AI api key, base URL, and model must be non-empty".to_owned(),
            ));
        }
        Ok(config)
    }

    fn completions_url(&self) -> String {
        if self.base_url.ends_with("/chat/completions") {
            self.base_url.clone()
        } else {
            format!("{}/chat/completions", self.base_url)
        }
    }
}

fn load_file_config(
    explicit_path: Option<&Path>,
    dotenv_path: Option<&Path>,
) -> Result<AiFileConfig, AiProviderError> {
    if let Some(path) = explicit_path {
        return read_file_config(path);
    }
    let Some(path) = default_config_path(dotenv_path) else {
        return Ok(AiFileConfig::default());
    };
    read_file_config(&path)
}

fn default_config_path(dotenv_path: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = dotenv_path
        .and_then(Path::parent)
        .map(|root| root.join("config/ai.toml"))
        .filter(|path| path.is_file())
    {
        return Some(path);
    }
    if let Ok(current_dir) = std::env::current_dir()
        && let Some(path) = find_config_in_ancestors(&current_dir)
    {
        return Some(path);
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../config/ai.toml")
        .is_file()
        .then(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/ai.toml"))
}

fn find_config_in_ancestors(start: &Path) -> Option<PathBuf> {
    start.ancestors().find_map(|root| {
        let path = root.join("config/ai.toml");
        path.is_file().then_some(path)
    })
}

fn read_file_config(path: &Path) -> Result<AiFileConfig, AiProviderError> {
    let contents = fs::read_to_string(path).map_err(|error| {
        AiProviderError::InvalidConfiguration(format!(
            "unable to read AI config {}: {error}",
            path.display()
        ))
    })?;
    toml::from_str(&contents).map_err(|error| {
        AiProviderError::InvalidConfiguration(format!(
            "invalid AI config {}: {error}",
            path.display()
        ))
    })
}

fn default_model_for_base_url(base_url: &str) -> &'static str {
    if base_url.to_ascii_lowercase().contains("deepseek.com") {
        "deepseek-v4-flash"
    } else {
        "gpt-4o-mini"
    }
}

fn first_non_empty_env(names: &[&str]) -> Option<String> {
    first_non_empty(names.iter().map(|name| std::env::var(name).ok()))
}

fn first_non_empty(values: impl IntoIterator<Item = Option<String>>) -> Option<String> {
    values.into_iter().find_map(|value| {
        value
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    })
}

pub struct OpenAiCompatibleProvider {
    config: OpenAiCompatibleConfig,
    client: reqwest::blocking::Client,
}

impl OpenAiCompatibleProvider {
    pub fn new(config: OpenAiCompatibleConfig) -> Result<Self, AiProviderError> {
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|error| AiProviderError::InvalidConfiguration(error.to_string()))?;
        Ok(Self { config, client })
    }

    pub fn from_env() -> Result<Self, AiProviderError> {
        Self::new(OpenAiCompatibleConfig::from_env()?)
    }
}

impl AiProvider for OpenAiCompatibleProvider {
    fn id(&self) -> &str {
        "openai-compatible"
    }

    fn stream(
        &self,
        request: AiProviderRequest,
        sender: async_channel::Sender<AiStreamEvent>,
        cancellation: AiCancellationToken,
    ) -> Result<(), AiProviderError> {
        if cancellation.is_cancelled() {
            return Err(AiProviderError::Cancelled);
        }
        let response = self
            .client
            .post(self.config.completions_url())
            .bearer_auth(&self.config.api_key)
            .json(&request_body(&self.config.model, &request))
            .send()
            .map_err(|error| AiProviderError::Request(error.to_string()))?;
        if !response.status().is_success() {
            let status = response.status();
            let message = response
                .text()
                .unwrap_or_else(|_| "unable to read response body".to_owned());
            return Err(AiProviderError::Request(format!(
                "AI provider returned {status}: {}",
                truncate_error(&message)
            )));
        }

        let mut saw_done = false;
        let mut buffered = String::new();
        let mut last_flush = Instant::now();
        for line in BufReader::new(response).lines() {
            if cancellation.is_cancelled() {
                return Err(AiProviderError::Cancelled);
            }
            let line = line.map_err(|error| AiProviderError::Protocol(error.to_string()))?;
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data == "[DONE]" {
                saw_done = true;
                break;
            }
            if data.is_empty() {
                continue;
            }
            let event: Value = serde_json::from_str(data)
                .map_err(|error| AiProviderError::Protocol(error.to_string()))?;
            if let Some(message) = event
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
            {
                return Err(AiProviderError::Request(message.to_owned()));
            }
            if let Some(text) = event
                .pointer("/choices/0/delta/content")
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
            {
                buffered.push_str(text);
                if last_flush.elapsed() >= Duration::from_millis(32) {
                    send_stream_event(
                        &sender,
                        AiStreamEvent::Delta {
                            request_id: request.request_id,
                            text: std::mem::take(&mut buffered),
                        },
                        &cancellation,
                    )?;
                    last_flush = Instant::now();
                }
            }
        }
        if !saw_done && cancellation.is_cancelled() {
            return Err(AiProviderError::Cancelled);
        }
        if !buffered.is_empty() {
            send_stream_event(
                &sender,
                AiStreamEvent::Delta {
                    request_id: request.request_id,
                    text: buffered,
                },
                &cancellation,
            )?;
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

fn request_body(model: &str, request: &AiProviderRequest) -> Value {
    let task = match request.task {
        AiTaskKind::InlineCompletion => "Continue at the caret. Return only inserted text.",
        AiTaskKind::RewriteSelection => "Rewrite the selection. Return only replacement text.",
        AiTaskKind::RewriteBlocks => {
            "Rewrite the selected blocks. Preserve paragraph boundaries and return only replacement text."
        }
    };
    json!({
        "model": model,
        "stream": true,
        "messages": [
            {
                "role": "system",
                "content": "You are an inline writing assistant embedded in a rich text editor. Do not include commentary, labels, or markdown fences unless requested."
            },
            {
                "role": "user",
                "content": format!(
                    "Task: {task}\nInstruction: {}\nSelected text:\n{}\nContext before:\n{}\nContext after:\n{}",
                    request.instruction,
                    request.selected_text,
                    request.prefix,
                    request.suffix,
                )
            }
        ]
    })
}

fn truncate_error(message: &str) -> String {
    const MAX: usize = 512;
    if message.len() <= MAX {
        return message.to_owned();
    }
    let mut end = MAX;
    while end > 0 && !message.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &message[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_normalizes_base_url_and_redacts_api_key() {
        let config =
            OpenAiCompatibleConfig::new(" secret ", "https://example.test/v1/", "model").unwrap();
        assert_eq!(
            config.completions_url(),
            "https://example.test/v1/chat/completions"
        );
        assert!(!format!("{config:?}").contains("secret"));
    }

    #[test]
    fn request_body_requires_replacement_only() {
        let request = AiProviderRequest {
            request_id: 1,
            task: AiTaskKind::RewriteSelection,
            instruction: "Improve writing".to_owned(),
            selected_text: "draft".to_owned(),
            prefix: "before".to_owned(),
            suffix: "after".to_owned(),
        };
        let body = request_body("model", &request);
        assert_eq!(body["stream"], true);
        assert!(
            body["messages"][1]["content"]
                .as_str()
                .unwrap()
                .contains("Return only replacement text")
        );
    }

    #[test]
    fn first_non_empty_env_skips_empty_aliases() {
        assert_eq!(
            first_non_empty([
                Some("  ".to_owned()),
                Some(" token ".to_owned()),
                Some("fallback".to_owned()),
            ]),
            Some("token".to_owned())
        );
        assert_eq!(first_non_empty([None, Some("".to_owned())]), None);
    }

    #[test]
    fn repository_config_selects_supported_deepseek_model() {
        let config: AiFileConfig = toml::from_str(include_str!("../../../config/ai.toml")).unwrap();
        assert_eq!(config.base_url.as_deref(), Some("https://api.deepseek.com"));
        assert_eq!(config.model.as_deref(), Some("deepseek-v4-flash"));
        assert!(toml::from_str::<AiFileConfig>("api_key = \"must-not-live-here\"").is_err());
    }

    #[test]
    fn provider_defaults_follow_base_url() {
        assert_eq!(
            default_model_for_base_url("https://api.deepseek.com"),
            "deepseek-v4-flash"
        );
        assert_eq!(
            default_model_for_base_url("https://api.openai.com/v1"),
            "gpt-4o-mini"
        );
    }
}
