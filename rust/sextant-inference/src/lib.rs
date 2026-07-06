use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum InferenceBackend {
    LlamaCpp,  // Industry standard for CPU/Lightweight GPU
    VLLM,      // High-throughput production GPU serving (PagedAttention)
    Ollama,    // Standardized CLI/API wrapper for llama.cpp
    MLCLLM,    // Compiler-driven cross-hardware (AMD/Mobile/WebGPU)
    OpenAI,    // ChatGPT / GPT-4o
    Anthropic, // Claude 3.5 Sonnet
    Gemini,    // Gemini 1.5 Pro/Flash
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum InferenceTask {
    Generate,
    Embed,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InferenceConfig {
    pub backend: InferenceBackend,
    pub endpoint: Url,
    pub model_name: String,
    pub api_key: Option<String>,
    pub temperature: f32,
    pub max_tokens: u32,
}

#[async_trait]
pub trait InferenceProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn generate(&self, prompt: &str, config: &InferenceConfig) -> Result<String, String>;
}

pub struct SextantInference {
    client: Client,
}

/// Default request timeout. Without one, a stalled model server hangs the
/// pilot lane indefinitely. Overridable via `SEXTANT_INFERENCE_TIMEOUT_SECS`.
const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 120;

impl SextantInference {
    pub fn new() -> Self {
        let timeout_secs = std::env::var("SEXTANT_INFERENCE_TIMEOUT_SECS")
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_REQUEST_TIMEOUT_SECS);
        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self { client }
    }

    /// Surfaces HTTP-level API errors (auth failures, quota, bad model names)
    /// instead of letting them decay into a generic "Invalid response from X"
    /// JSON-shape error.
    async fn json_checked(
        res: reqwest::Response,
        provider: &str,
    ) -> Result<serde_json::Value, String> {
        let status = res.status();
        if !status.is_success() {
            let body = res.text().await.unwrap_or_default();
            let snippet: String = body.chars().take(300).collect();
            return Err(format!("{provider} returned HTTP {status}: {snippet}"));
        }
        res.json().await.map_err(|e| e.to_string())
    }

    pub async fn generate(&self, prompt: &str, config: &InferenceConfig) -> Result<String, String> {
        match config.backend {
            InferenceBackend::LlamaCpp => self.generate_llama_cpp(prompt, config).await,
            InferenceBackend::VLLM => self.generate_vllm(prompt, config).await,
            InferenceBackend::Ollama => self.generate_ollama(prompt, config).await,
            InferenceBackend::MLCLLM => self.generate_mlc_llm(prompt, config).await,
            InferenceBackend::OpenAI => self.generate_openai(prompt, config).await,
            InferenceBackend::Anthropic => self.generate_anthropic(prompt, config).await,
            InferenceBackend::Gemini => self.generate_gemini(prompt, config).await,
        }
    }

    pub async fn embed(&self, text: &str, config: &InferenceConfig) -> Result<Vec<f32>, String> {
        match config.backend {
            InferenceBackend::OpenAI => self.embed_openai(text, config).await,
            InferenceBackend::Gemini => self.embed_gemini(text, config).await,
            InferenceBackend::Ollama => self.embed_ollama(text, config).await,
            _ => Err(format!(
                "Embedding not supported for backend {:?}",
                config.backend
            )),
        }
    }

    async fn embed_openai(&self, text: &str, config: &InferenceConfig) -> Result<Vec<f32>, String> {
        let url = "https://api.openai.com/v1/embeddings";
        let api_key = config.api_key.as_ref().ok_or("OpenAI API key missing")?;

        let body = serde_json::json!({
            "model": "text-embedding-3-small",
            "input": text,
        });

        let res = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let json = Self::json_checked(res, "OpenAI").await?;
        let embedding = json["data"][0]["embedding"]
            .as_array()
            .ok_or("Invalid response from OpenAI")?
            .iter()
            .map(|v| v.as_f64().unwrap_or(0.0) as f32)
            .collect::<Vec<f32>>();
        Ok(embedding)
    }

    async fn embed_gemini(&self, text: &str, config: &InferenceConfig) -> Result<Vec<f32>, String> {
        let api_key = config.api_key.as_ref().ok_or("Gemini API key missing")?;
        // The key travels in a header, not the query string: URLs are logged
        // by proxies and tracing layers, headers generally are not.
        let url = "https://generativelanguage.googleapis.com/v1beta/models/text-embedding-004:embedContent";

        let body = serde_json::json!({
            "model": "models/text-embedding-004",
            "content": {
                "parts": [{
                    "text": text
                }]
            }
        });

        let res = self
            .client
            .post(url)
            .header("x-goog-api-key", api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let json = Self::json_checked(res, "Gemini").await?;
        let embedding = json["embedding"]["values"]
            .as_array()
            .ok_or("Invalid response from Gemini")?
            .iter()
            .map(|v| v.as_f64().unwrap_or(0.0) as f32)
            .collect::<Vec<f32>>();
        Ok(embedding)
    }

    async fn embed_ollama(&self, text: &str, config: &InferenceConfig) -> Result<Vec<f32>, String> {
        let url = config
            .endpoint
            .join("/api/embeddings")
            .map_err(|e| e.to_string())?;
        let body = serde_json::json!({
            "model": config.model_name,
            "prompt": text,
        });

        let res = self
            .client
            .post(url)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let json = Self::json_checked(res, "Ollama").await?;
        let embedding = json["embedding"]
            .as_array()
            .ok_or("Invalid response from Ollama")?
            .iter()
            .map(|v| v.as_f64().unwrap_or(0.0) as f32)
            .collect::<Vec<f32>>();
        Ok(embedding)
    }

    async fn generate_llama_cpp(
        &self,
        prompt: &str,
        config: &InferenceConfig,
    ) -> Result<String, String> {
        // Use llama.cpp's OpenAI-compatible chat endpoint, which applies the
        // model's chat template. Instruct models need that to follow
        // instructions; the raw /completion path feeds an unwrapped prompt and
        // produces unreliable (often unparseable) output.
        let url = config
            .endpoint
            .join("/v1/chat/completions")
            .map_err(|e| e.to_string())?;
        let body = serde_json::json!({
            "model": config.model_name,
            "messages": [{ "role": "user", "content": prompt }],
            "temperature": config.temperature,
            "max_tokens": config.max_tokens,
        });

        let res = self
            .client
            .post(url)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let json = Self::json_checked(res, "llama.cpp").await?;
        json["choices"][0]["message"]["content"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or("Invalid response from llama.cpp".into())
    }

    async fn generate_vllm(
        &self,
        prompt: &str,
        config: &InferenceConfig,
    ) -> Result<String, String> {
        // vLLM OpenAI-compatible endpoint
        let url = config
            .endpoint
            .join("/v1/completions")
            .map_err(|e| e.to_string())?;
        let body = serde_json::json!({
            "model": config.model_name,
            "prompt": prompt,
            "temperature": config.temperature,
            "max_tokens": config.max_tokens,
        });

        let res = self
            .client
            .post(url)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let json = Self::json_checked(res, "vLLM").await?;
        json["choices"][0]["text"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or("Invalid response from vLLM".into())
    }

    async fn generate_ollama(
        &self,
        prompt: &str,
        config: &InferenceConfig,
    ) -> Result<String, String> {
        // Ollama /api/generate endpoint
        let url = config
            .endpoint
            .join("/api/generate")
            .map_err(|e| e.to_string())?;
        let body = serde_json::json!({
            "model": config.model_name,
            "prompt": prompt,
            "stream": false,
            "options": {
                "temperature": config.temperature,
                "num_predict": config.max_tokens
            }
        });

        let res = self
            .client
            .post(url)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let json = Self::json_checked(res, "Ollama").await?;
        json["response"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or("Invalid response from Ollama".into())
    }

    async fn generate_mlc_llm(
        &self,
        prompt: &str,
        config: &InferenceConfig,
    ) -> Result<String, String> {
        // MLC-LLM often uses OpenAI-compatible REST API when serving
        let url = config
            .endpoint
            .join("/v1/chat/completions")
            .map_err(|e| e.to_string())?;
        let body = serde_json::json!({
            "model": config.model_name,
            "messages": [{"role": "user", "content": prompt}],
            "temperature": config.temperature,
            "max_tokens": config.max_tokens,
        });

        let res = self
            .client
            .post(url)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let json = Self::json_checked(res, "MLC-LLM").await?;
        json["choices"][0]["message"]["content"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or("Invalid response from MLC-LLM".into())
    }

    async fn generate_openai(
        &self,
        prompt: &str,
        config: &InferenceConfig,
    ) -> Result<String, String> {
        let url = "https://api.openai.com/v1/chat/completions";
        let api_key = config.api_key.as_ref().ok_or("OpenAI API key missing")?;

        let body = serde_json::json!({
            "model": config.model_name,
            "messages": [{"role": "user", "content": prompt}],
            "temperature": config.temperature,
            "max_tokens": config.max_tokens,
        });

        let res = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let json = Self::json_checked(res, "OpenAI").await?;
        json["choices"][0]["message"]["content"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or("Invalid response from OpenAI".into())
    }

    async fn generate_anthropic(
        &self,
        prompt: &str,
        config: &InferenceConfig,
    ) -> Result<String, String> {
        let url = "https://api.anthropic.com/v1/messages";
        let api_key = config.api_key.as_ref().ok_or("Anthropic API key missing")?;

        let body = serde_json::json!({
            "model": config.model_name,
            "max_tokens": config.max_tokens,
            "messages": [{"role": "user", "content": prompt}],
            "temperature": config.temperature,
        });

        let res = self
            .client
            .post(url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let json = Self::json_checked(res, "Anthropic").await?;
        json["content"][0]["text"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or("Invalid response from Anthropic".into())
    }

    async fn generate_gemini(
        &self,
        prompt: &str,
        config: &InferenceConfig,
    ) -> Result<String, String> {
        let api_key = config.api_key.as_ref().ok_or("Gemini API key missing")?;
        // Key in header, not URL query (see embed_gemini).
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
            config.model_name
        );

        let body = serde_json::json!({
            "contents": [{
                "parts": [{
                    "text": prompt
                }]
            }],
            "generationConfig": {
                "temperature": config.temperature,
                "maxOutputTokens": config.max_tokens,
            }
        });

        let res = self
            .client
            .post(url)
            .header("x-goog-api-key", api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let json = Self::json_checked(res, "Gemini").await?;
        json["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or("Invalid response from Gemini".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(backend: InferenceBackend, api_key: Option<String>) -> InferenceConfig {
        InferenceConfig {
            backend,
            endpoint: Url::parse("http://127.0.0.1:9").unwrap(),
            model_name: "test-model".to_string(),
            api_key,
            temperature: 0.0,
            max_tokens: 8,
        }
    }

    #[tokio::test]
    async fn cloud_generation_requires_configured_api_key_before_network() {
        let inference = SextantInference::new();

        let openai = inference
            .generate("hello", &config(InferenceBackend::OpenAI, None))
            .await;
        let anthropic = inference
            .generate("hello", &config(InferenceBackend::Anthropic, None))
            .await;
        let gemini = inference
            .generate("hello", &config(InferenceBackend::Gemini, None))
            .await;

        assert_eq!(openai, Err("OpenAI API key missing".to_string()));
        assert_eq!(anthropic, Err("Anthropic API key missing".to_string()));
        assert_eq!(gemini, Err("Gemini API key missing".to_string()));
    }

    #[tokio::test]
    async fn unsupported_embedding_backend_is_explicit() {
        let inference = SextantInference::new();
        let result = inference
            .embed("hello", &config(InferenceBackend::LlamaCpp, None))
            .await;

        assert_eq!(
            result,
            Err("Embedding not supported for backend LlamaCpp".to_string())
        );
    }
}
