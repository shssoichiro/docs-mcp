use super::*;
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
