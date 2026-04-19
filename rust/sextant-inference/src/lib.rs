use serde::{Serialize, Deserialize};
use async_trait::async_trait;
use reqwest::Client;
use url::Url;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum InferenceBackend {
    LlamaCpp,   // Industry standard for CPU/Lightweight GPU
    VLLM,       // High-throughput production GPU serving (PagedAttention)
    Ollama,     // Standardized CLI/API wrapper for llama.cpp
    MLCLLM,     // Compiler-driven cross-hardware (AMD/Mobile/WebGPU)
    OpenAI,     // ChatGPT / GPT-4o
    Anthropic,  // Claude 3.5 Sonnet
    Gemini,     // Gemini 1.5 Pro/Flash
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

impl SextantInference {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
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
            _ => Err(format!("Embedding not supported for backend {:?}", config.backend)),
        }
    }

    async fn embed_openai(&self, text: &str, config: &InferenceConfig) -> Result<Vec<f32>, String> {
        let url = "https://api.openai.com/v1/embeddings";
        let api_key = config.api_key.as_ref().ok_or("OpenAI API key missing")?;
        
        let body = serde_json::json!({
            "model": "text-embedding-3-small",
            "input": text,
        });

        let res = self.client.post(url)
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let json: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
        let embedding = json["data"][0]["embedding"].as_array()
            .ok_or("Invalid response from OpenAI")?
            .iter()
            .map(|v| v.as_f64().unwrap_or(0.0) as f32)
            .collect::<Vec<f32>>();
        Ok(embedding)
    }

    async fn embed_gemini(&self, text: &str, config: &InferenceConfig) -> Result<Vec<f32>, String> {
        let api_key = config.api_key.as_ref().ok_or("Gemini API key missing")?;
        let url = format!("https://generativelanguage.googleapis.com/v1beta/models/text-embedding-004:embedContent?key={}", api_key);
        
        let body = serde_json::json!({
            "model": "models/text-embedding-004",
            "content": {
                "parts": [{
                    "text": text
                }]
            }
        });

        let res = self.client.post(url)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let json: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
        let embedding = json["embedding"]["values"].as_array()
            .ok_or("Invalid response from Gemini")?
            .iter()
            .map(|v| v.as_f64().unwrap_or(0.0) as f32)
            .collect::<Vec<f32>>();
        Ok(embedding)
    }

    async fn embed_ollama(&self, text: &str, config: &InferenceConfig) -> Result<Vec<f32>, String> {
        let url = config.endpoint.join("/api/embeddings").map_err(|e| e.to_string())?;
        let body = serde_json::json!({
            "model": config.model_name,
            "prompt": text,
        });

        let res = self.client.post(url)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let json: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
        let embedding = json["embedding"].as_array()
            .ok_or("Invalid response from Ollama")?
            .iter()
            .map(|v| v.as_f64().unwrap_or(0.0) as f32)
            .collect::<Vec<f32>>();
        Ok(embedding)
    }

    async fn generate_llama_cpp(&self, prompt: &str, config: &InferenceConfig) -> Result<String, String> {
        // llama.cpp /completion endpoint
        let url = config.endpoint.join("/completion").map_err(|e| e.to_string())?;
        let body = serde_json::json!({
            "prompt": prompt,
            "temperature": config.temperature,
            "n_predict": config.max_tokens,
        });

        let res = self.client.post(url)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let json: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
        json["content"].as_str().map(|s| s.to_string()).ok_or("Invalid response from llama.cpp".into())
    }

    async fn generate_vllm(&self, prompt: &str, config: &InferenceConfig) -> Result<String, String> {
        // vLLM OpenAI-compatible endpoint
        let url = config.endpoint.join("/v1/completions").map_err(|e| e.to_string())?;
        let body = serde_json::json!({
            "model": config.model_name,
            "prompt": prompt,
            "temperature": config.temperature,
            "max_tokens": config.max_tokens,
        });

        let res = self.client.post(url)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let json: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
        json["choices"][0]["text"].as_str().map(|s| s.to_string()).ok_or("Invalid response from vLLM".into())
    }

    async fn generate_ollama(&self, prompt: &str, config: &InferenceConfig) -> Result<String, String> {
        // Ollama /api/generate endpoint
        let url = config.endpoint.join("/api/generate").map_err(|e| e.to_string())?;
        let body = serde_json::json!({
            "model": config.model_name,
            "prompt": prompt,
            "stream": false,
            "options": {
                "temperature": config.temperature,
                "num_predict": config.max_tokens
            }
        });

        let res = self.client.post(url)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let json: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
        json["response"].as_str().map(|s| s.to_string()).ok_or("Invalid response from Ollama".into())
    }

    async fn generate_mlc_llm(&self, prompt: &str, config: &InferenceConfig) -> Result<String, String> {
        // MLC-LLM often uses OpenAI-compatible REST API when serving
        let url = config.endpoint.join("/v1/chat/completions").map_err(|e| e.to_string())?;
        let body = serde_json::json!({
            "model": config.model_name,
            "messages": [{"role": "user", "content": prompt}],
            "temperature": config.temperature,
            "max_tokens": config.max_tokens,
        });

        let res = self.client.post(url)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let json: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
        json["choices"][0]["message"]["content"].as_str().map(|s| s.to_string()).ok_or("Invalid response from MLC-LLM".into())
    }

    async fn generate_openai(&self, prompt: &str, config: &InferenceConfig) -> Result<String, String> {
        let url = "https://api.openai.com/v1/chat/completions";
        let api_key = config.api_key.as_ref().ok_or("OpenAI API key missing")?;
        
        let body = serde_json::json!({
            "model": config.model_name,
            "messages": [{"role": "user", "content": prompt}],
            "temperature": config.temperature,
            "max_tokens": config.max_tokens,
        });

        let res = self.client.post(url)
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let json: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
        json["choices"][0]["message"]["content"].as_str().map(|s| s.to_string()).ok_or("Invalid response from OpenAI".into())
    }

    async fn generate_anthropic(&self, prompt: &str, config: &InferenceConfig) -> Result<String, String> {
        let url = "https://api.anthropic.com/v1/messages";
        let api_key = config.api_key.as_ref().ok_or("Anthropic API key missing")?;
        
        let body = serde_json::json!({
            "model": config.model_name,
            "max_tokens": config.max_tokens,
            "messages": [{"role": "user", "content": prompt}],
            "temperature": config.temperature,
        });

        let res = self.client.post(url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let json: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
        json["content"][0]["text"].as_str().map(|s| s.to_string()).ok_or("Invalid response from Anthropic".into())
    }

    async fn generate_gemini(&self, prompt: &str, config: &InferenceConfig) -> Result<String, String> {
        let api_key = config.api_key.as_ref().ok_or("Gemini API key missing")?;
        let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}", config.model_name, api_key);
        
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

        let res = self.client.post(url)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let json: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
        json["candidates"][0]["content"]["parts"][0]["text"].as_str().map(|s| s.to_string()).ok_or("Invalid response from Gemini".into())
    }
}
