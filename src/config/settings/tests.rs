use super::*;
use crate::embeddings::chunking::ChunkingConfig;
use tempfile::TempDir;

#[test]
fn config_validation() {
    let config = OllamaConfig::default();
    assert!(config.validate().is_ok());

    let mut invalid_config = config.clone();
    invalid_config.protocol = "ftp".to_string();
    assert!(invalid_config.validate().is_err());

    let mut invalid_config = config.clone();
    invalid_config.port = 0;
    assert!(invalid_config.validate().is_err());

    let mut invalid_config = config.clone();
    invalid_config.model = String::new();
    assert!(invalid_config.validate().is_err());

    let mut invalid_config = config.clone();
    invalid_config.batch_size = 0;
    assert!(invalid_config.validate().is_err());

    let mut invalid_config = config;
    invalid_config.batch_size = 1001;
    assert!(invalid_config.validate().is_err());
}

#[test]
fn ollama_url_generation() {
    let config = OllamaConfig::default();
    let url = config
        .ollama_url()
        .expect("should generate ollama_url successfully");
    assert_eq!(url.as_str(), "http://localhost:11434/");
}

#[test]
fn setter_validation() {
    let mut config = OllamaConfig {
        protocol: "http".to_string(),
        host: "localhost".to_string(),
        port: 11434,
        model: "test-model".to_string(),
        batch_size: 32,
        embedding_dimension: None,
    };

    assert!(config.set_protocol("https".to_string()).is_ok());
    assert!(config.set_host("example.com".to_string()).is_ok());
    assert!(config.set_port(8080).is_ok());
    assert!(config.set_model("new-model".to_string()).is_ok());
    assert!(config.set_batch_size(128).is_ok());

    assert!(config.set_protocol("ftp".to_string()).is_err());
    assert!(config.set_port(0).is_err());
    assert!(config.set_model(String::new()).is_err());
    assert!(config.set_batch_size(0).is_err());
    assert!(config.set_batch_size(1001).is_err());
}

#[test]
fn load_missing_config() {
    let temp_dir = TempDir::new().expect("should create temp dir");

    // Create a config with the temp directory as base_dir
    let config = Config::load(temp_dir.path()).expect("config created");

    assert!(config.validate().is_ok());
    assert_eq!(config.ollama.protocol, "http");
    assert_eq!(config.ollama.host, "localhost");
    assert_eq!(config.ollama.port, 11434);
}

#[test]
fn https_url_generation() {
    let config = OllamaConfig {
        protocol: "https".to_string(),
        host: "secure.example.com".to_string(),
        port: 443,
        ..Default::default()
    };

    let url = config
        .ollama_url()
        .expect("should generate https url successfully");
    assert_eq!(url.as_str(), "https://secure.example.com/");
}

#[test]
fn protocol_validation() {
    let mut config = OllamaConfig {
        protocol: "http".to_string(),
        host: "localhost".to_string(),
        port: 11434,
        model: "test-model".to_string(),
        batch_size: 32,
        embedding_dimension: None,
    };

    // Valid protocols
    assert!(config.set_protocol("http".to_string()).is_ok());
    assert!(config.set_protocol("https".to_string()).is_ok());

    // Invalid protocols
    assert!(config.set_protocol("ftp".to_string()).is_err());
    assert!(config.set_protocol("ws".to_string()).is_err());
    assert!(config.set_protocol(String::new()).is_err());
    assert!(config.set_protocol("HTTP".to_string()).is_err()); // case sensitive
}

#[test]
fn chunking_config_validation() {
    // Valid default config
    let config = Config {
        ollama: OllamaConfig::default(),
        chunking: ChunkingConfig::default(),
        base_dir: PathBuf::from("/tmp"),
    };
    assert!(config.validate().is_ok());

    // Invalid target chunk size - too small
    let mut invalid_config = config.clone();
    invalid_config.chunking.target_chunk_size = 50;
    assert!(invalid_config.validate().is_err());

    // Invalid target chunk size - too large
    let mut invalid_config = config.clone();
    invalid_config.chunking.target_chunk_size = 3000;
    assert!(invalid_config.validate().is_err());

    // Invalid max chunk size - too small
    let mut invalid_config = config.clone();
    invalid_config.chunking.max_chunk_size = 150;
    assert!(invalid_config.validate().is_err());

    // Invalid max chunk size - too large
    let mut invalid_config = config.clone();
    invalid_config.chunking.max_chunk_size = 5000;
    assert!(invalid_config.validate().is_err());

    // Invalid min chunk size - too small
    let mut invalid_config = config.clone();
    invalid_config.chunking.min_chunk_size = 20;
    assert!(invalid_config.validate().is_err());

    // Invalid min chunk size - too large
    let mut invalid_config = config.clone();
    invalid_config.chunking.min_chunk_size = 1500;
    assert!(invalid_config.validate().is_err());

    // Invalid overlap size - too large
    let mut invalid_config = config.clone();
    invalid_config.chunking.overlap_size = 600;
    assert!(invalid_config.validate().is_err());

    // Invalid relationship: max <= target
    let mut invalid_config = config.clone();
    invalid_config.chunking.target_chunk_size = 800;
    invalid_config.chunking.max_chunk_size = 800;
    assert!(invalid_config.validate().is_err());

    // Invalid relationship: target <= min
    let mut invalid_config = config.clone();
    invalid_config.chunking.target_chunk_size = 200;
    invalid_config.chunking.min_chunk_size = 200;
    assert!(invalid_config.validate().is_err());

    // Valid custom config
    let mut valid_config = config;
    valid_config.chunking.target_chunk_size = 500;
    valid_config.chunking.max_chunk_size = 800;
    valid_config.chunking.min_chunk_size = 100;
    valid_config.chunking.overlap_size = 50;
    assert!(valid_config.validate().is_ok());
}

#[test]
fn config_toml_serialization() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let config = Config {
        ollama: OllamaConfig::default(),
        chunking: ChunkingConfig::default(),
        base_dir: temp_dir.path().to_path_buf(),
    };

    // Should save successfully
    assert!(config.save().is_ok());

    // Should load and match original
    let loaded_config = Config::load(temp_dir.path()).expect("should load config");
    assert_eq!(loaded_config.ollama, config.ollama);
    assert_eq!(loaded_config.chunking, config.chunking);
}

#[test]
fn config_toml_partial_chunking_config() {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let config_path = temp_dir.path().join("config.toml");

    // Write partial config (missing chunking section)
    let partial_toml = r#"
[ollama]
protocol = "http"
host = "localhost"
port = 11434
model = "nomic-embed-text:latest"
batch_size = 64
"#;
    std::fs::write(&config_path, partial_toml).expect("should write partial config");

    // Should load successfully with chunking defaults
    let loaded_config = Config::load(temp_dir.path()).expect("should load partial config");
    assert_eq!(loaded_config.chunking, ChunkingConfig::default());

    // Write config with custom chunking
    let custom_toml = r#"
[ollama]
protocol = "http"
host = "localhost"
port = 11434
model = "nomic-embed-text:latest"
batch_size = 64

[chunking]
target_chunk_size = 500
max_chunk_size = 800
min_chunk_size = 100
overlap_size = 50
preserve_code_blocks = false
sentence_boundary_splitting = false
"#;
    std::fs::write(&config_path, custom_toml).expect("should write custom config");

    let loaded_config = Config::load(temp_dir.path()).expect("should load custom config");
    assert_eq!(loaded_config.chunking.target_chunk_size, 500);
    assert_eq!(loaded_config.chunking.max_chunk_size, 800);
    assert_eq!(loaded_config.chunking.min_chunk_size, 100);
    assert_eq!(loaded_config.chunking.overlap_size, 50);
    assert!(!loaded_config.chunking.preserve_code_blocks);
    assert!(!loaded_config.chunking.sentence_boundary_splitting);
}
