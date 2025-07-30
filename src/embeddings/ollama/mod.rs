#[cfg(test)]
mod tests;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, error, info, warn};
use url::Url;

use crate::config::Config;
use crate::embeddings::chunking::ContentChunk;

const DEFAULT_TIMEOUT_SECONDS: u64 = 30;
const DEFAULT_RETRY_ATTEMPTS: u32 = 3;
const EXPONENTIAL_BACKOFF_BASE: u64 = 2;

#[derive(Debug, Clone)]
pub struct OllamaClient {
    base_url: Url,
    model: String,
    batch_size: u32,
    agent: ureq::Agent,
    retry_attempts: u32,
}

#[derive(Debug, Serialize)]
struct EmbedRequest {
    model: String,
    prompt: String,
}

#[derive(Debug, Serialize)]
struct BatchEmbedRequest {
    model: String,
    #[serde(rename = "input")]
    inputs: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct EmbedResponse {
    embedding: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct BatchEmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

#[derive(Debug, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub size: Option<u64>,
    pub digest: Option<String>,
    pub details: Option<ModelDetails>,
}

#[derive(Debug, Deserialize)]
pub struct ModelDetails {
    pub format: Option<String>,
    pub family: Option<String>,
    pub families: Option<Vec<String>>,
    pub parameter_size: Option<String>,
    pub quantization_level: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    models: Vec<ModelInfo>,
}

#[derive(Debug, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddingResult {
    pub text: String,
    pub embedding: Vec<f32>,
    pub token_count: usize,
    pub chunk_index: Option<usize>,
    pub heading_path: Option<String>,
}

impl OllamaClient {
    #[inline]
    pub fn new(config: &Config) -> Result<Self> {
        let base_url = config
            .ollama_url()
            .context("Failed to generate Ollama URL from config")?;

        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(DEFAULT_TIMEOUT_SECONDS)))
            .build()
            .into();

        Ok(Self {
            base_url,
            model: config.ollama.model.clone(),
            batch_size: config.ollama.batch_size,
            agent,
            retry_attempts: DEFAULT_RETRY_ATTEMPTS,
        })
    }

    #[inline]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.agent = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .build()
            .into();
        self
    }

    #[inline]
    pub fn with_retry_attempts(mut self, attempts: u32) -> Self {
        self.retry_attempts = attempts;
        self
    }

    /// Test connection to Ollama server and verify model availability
    #[inline]
    pub fn health_check(&self) -> Result<()> {
        debug!("Performing health check for Ollama at {}", self.base_url);

        // First check if server is reachable
        self.ping().context("Server ping failed")?;

        // Then check if model is available
        self.validate_model().context("Model validation failed")?;

        info!(
            "Health check passed for Ollama server at {} with model {}",
            self.base_url, self.model
        );
        Ok(())
    }

    /// Ping the Ollama server to check if it's responsive
    #[inline]
    pub fn ping(&self) -> Result<()> {
        let url = self
            .base_url
            .join("/api/tags")
            .context("Failed to build ping URL")?;

        debug!("Pinging Ollama server at {}", url);

        self.make_request_with_retry(|| {
            self.agent
                .get(url.as_str())
                .call()
                .and_then(|mut resp| resp.body_mut().read_to_string())
        })
        .context("Failed to ping Ollama server")?;

        debug!("Server ping successful");
        Ok(())
    }

    /// Validate that the configured model is available
    #[inline]
    pub fn validate_model(&self) -> Result<()> {
        debug!("Validating model: {}", self.model);

        let models = self.list_models().context("Failed to list models")?;

        if models.iter().any(|m| m.name == self.model) {
            debug!("Model {} is available", self.model);
            Ok(())
        } else {
            let available_models: Vec<&str> = models.iter().map(|m| m.name.as_str()).collect();
            warn!(
                "Model {} not found. Available models: {:?}",
                self.model, available_models
            );
            Err(anyhow::anyhow!(
                "Model '{}' is not available. Available models: {:?}",
                self.model,
                available_models
            ))
        }
    }

    /// List all available models
    #[inline]
    pub fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let url = self
            .base_url
            .join("/api/tags")
            .context("Failed to build models URL")?;

        debug!("Fetching available models from {}", url);

        let response_text = self
            .make_request_with_retry(|| {
                self.agent
                    .get(url.as_str())
                    .call()
                    .and_then(|mut resp| resp.body_mut().read_to_string())
            })
            .context("Failed to fetch models")?;

        let models_response: ModelsResponse =
            serde_json::from_str(&response_text).context("Failed to parse models response")?;

        debug!("Found {} models", models_response.models.len());
        Ok(models_response.models)
    }

    /// Generate embeddings for a single text input
    #[inline]
    pub fn generate_embedding(&self, text: &str) -> Result<EmbeddingResult> {
        debug!("Generating embedding for text (length: {})", text.len());

        let request = EmbedRequest {
            model: self.model.clone(),
            prompt: text.to_string(),
        };

        let url = self
            .base_url
            .join("/api/embed")
            .context("Failed to build embedding URL")?;

        let request_json =
            serde_json::to_string(&request).context("Failed to serialize embedding request")?;

        let response_text = self
            .make_request_with_retry(|| {
                self.agent
                    .post(url.as_str())
                    .header("Content-Type", "application/json")
                    .send(&request_json)
                    .and_then(|mut resp| resp.body_mut().read_to_string())
            })
            .context("Failed to generate embedding")?;

        let embed_response: EmbedResponse =
            serde_json::from_str(&response_text).context("Failed to parse embedding response")?;

        let result = EmbeddingResult {
            text: text.to_string(),
            embedding: embed_response.embedding,
            token_count: crate::embeddings::chunking::estimate_token_count(text),
            chunk_index: None,
            heading_path: None,
        };

        debug!(
            "Generated embedding with {} dimensions",
            result.embedding.len()
        );

        Ok(result)
    }

    /// Generate embeddings for multiple text inputs using batch processing
    #[inline]
    pub fn generate_embeddings_batch(&self, texts: &[String]) -> Result<Vec<EmbeddingResult>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        debug!("Generating embeddings for {} texts", texts.len());

        let mut results = Vec::with_capacity(texts.len());

        // Process in batches to avoid overwhelming the server
        for chunk in texts.chunks(self.batch_size as usize) {
            let batch_results = self
                .generate_embeddings_single_batch(chunk)
                .with_context(|| format!("Failed to process batch of {} texts", chunk.len()))?;

            results.extend(batch_results);
        }

        debug!("Generated {} embeddings total", results.len());
        Ok(results)
    }

    /// Generate embeddings for ContentChunk objects
    #[inline]
    pub fn generate_chunk_embeddings(
        &self,
        chunks: &[ContentChunk],
    ) -> Result<Vec<EmbeddingResult>> {
        if chunks.is_empty() {
            return Ok(Vec::new());
        }

        debug!("Generating embeddings for {} content chunks", chunks.len());

        let texts: Vec<String> = chunks.iter().map(|c| c.content.clone()).collect();
        let mut results = self.generate_embeddings_batch(&texts)?;

        // Add chunk-specific metadata to results
        for (result, chunk) in results.iter_mut().zip(chunks.iter()) {
            result.chunk_index = Some(chunk.chunk_index);
            result.heading_path = Some(chunk.heading_path.clone());
            result.token_count = chunk.token_count;
        }

        debug!("Enhanced embeddings with chunk metadata");
        Ok(results)
    }

    fn generate_embeddings_single_batch(&self, texts: &[String]) -> Result<Vec<EmbeddingResult>> {
        if texts.len() == 1 {
            // Use single embedding API for single text
            let result = self.generate_embedding(&texts[0])?;
            return Ok(vec![result]);
        }

        // Use batch API for multiple texts
        let request = BatchEmbedRequest {
            model: self.model.clone(),
            inputs: texts.to_vec(),
        };

        let url = self
            .base_url
            .join("/api/embed")
            .context("Failed to build batch embedding URL")?;

        let request_json = serde_json::to_string(&request)
            .context("Failed to serialize batch embedding request")?;

        let response_text = self
            .make_request_with_retry(|| {
                self.agent
                    .post(url.as_str())
                    .header("Content-Type", "application/json")
                    .send(&request_json)
                    .and_then(|mut resp| resp.body_mut().read_to_string())
            })
            .context("Failed to generate batch embeddings")?;

        let batch_response: BatchEmbedResponse = serde_json::from_str(&response_text)
            .context("Failed to parse batch embedding response")?;

        if batch_response.embeddings.len() != texts.len() {
            return Err(anyhow::anyhow!(
                "Mismatch between request and response counts: {} vs {}",
                texts.len(),
                batch_response.embeddings.len()
            ));
        }

        let results = texts
            .iter()
            .zip(batch_response.embeddings.iter())
            .map(|(text, embedding)| EmbeddingResult {
                text: text.clone(),
                embedding: embedding.clone(),
                token_count: crate::embeddings::chunking::estimate_token_count(text),
                chunk_index: None,
                heading_path: None,
            })
            .collect();

        Ok(results)
    }

    fn make_request_with_retry<F>(&self, mut request_fn: F) -> Result<String>
    where
        F: FnMut() -> Result<String, ureq::Error>,
    {
        let mut last_error = None;

        for attempt in 1..=self.retry_attempts {
            debug!("HTTP request attempt {}/{}", attempt, self.retry_attempts);

            match request_fn() {
                Ok(response_text) => {
                    debug!("Request succeeded on attempt {}", attempt);
                    return Ok(response_text);
                }
                Err(error) => {
                    let should_retry = match &error {
                        ureq::Error::StatusCode(status) => {
                            if *status >= 500 {
                                warn!(
                                    "Server error (status {}), attempt {}/{}",
                                    status, attempt, self.retry_attempts
                                );
                                true // Retry server errors
                            } else {
                                warn!("Client error (status {}), not retrying", status);
                                return Err(anyhow::anyhow!("Client error: HTTP {}", status));
                            }
                        }
                        ureq::Error::ConnectionFailed
                        | ureq::Error::HostNotFound
                        | ureq::Error::Timeout(_)
                        | ureq::Error::Io(_) => {
                            warn!(
                                "Transport error: {}, attempt {}/{}",
                                error, attempt, self.retry_attempts
                            );
                            true // Retry transport errors
                        }
                        _ => {
                            warn!("Non-retryable error: {}", error);
                            false // Don't retry other errors
                        }
                    };

                    if !should_retry {
                        return Err(anyhow::anyhow!("Non-retryable error: {}", error));
                    }

                    last_error = Some(anyhow::anyhow!("Request error: {}", error));

                    // Wait before retry (exponential backoff)
                    if attempt < self.retry_attempts {
                        let delay_ms = EXPONENTIAL_BACKOFF_BASE.pow(attempt - 1) * 1000;
                        let delay = Duration::from_millis(delay_ms);
                        debug!("Waiting {:?} before retry", delay);
                        std::thread::sleep(delay);
                    }
                }
            }
        }

        error!("All retry attempts failed for request to {}", self.base_url);

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Request failed after retries")))
    }
}
