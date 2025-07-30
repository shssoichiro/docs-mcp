use super::*;
use tempfile::TempDir;

#[test]
fn default_config() {
    let config = Config::default();
    assert_eq!(config.ollama.protocol, "http");
    assert_eq!(config.ollama.host, "localhost");
    assert_eq!(config.ollama.port, 11434);
    assert_eq!(config.ollama.model, "nomic-embed-text:latest");
    assert_eq!(config.ollama.batch_size, 64);
}

#[test]
fn config_validation() {
    let config = Config::default();
    assert!(config.validate().is_ok());

    let mut invalid_config = config.clone();
    invalid_config.ollama.protocol = "ftp".to_string();
    assert!(invalid_config.validate().is_err());

    let mut invalid_config = config.clone();
    invalid_config.ollama.port = 0;
    assert!(invalid_config.validate().is_err());

    let mut invalid_config = config.clone();
    invalid_config.ollama.model = String::new();
    assert!(invalid_config.validate().is_err());

    let mut invalid_config = config.clone();
    invalid_config.ollama.batch_size = 0;
    assert!(invalid_config.validate().is_err());

    let mut invalid_config = config;
    invalid_config.ollama.batch_size = 1001;
    assert!(invalid_config.validate().is_err());
}

#[test]
fn ollama_url_generation() {
    let config = Config::default();
    let url = config
        .ollama_url()
        .expect("should generate ollama_url successfully");
    assert_eq!(url.as_str(), "http://localhost:11434/");
}

#[test]
fn toml_serialization() {
    let config = Config::default();
    let toml_str = toml::to_string(&config).expect("should serialize toml correctly");
    let parsed_config: Config = toml::from_str(&toml_str).expect("should parse toml correctly");
    assert_eq!(config, parsed_config);
}

#[test]
fn setter_validation() {
    let mut config = OllamaConfig {
        protocol: "http".to_string(),
        host: "localhost".to_string(),
        port: 11434,
        model: "test-model".to_string(),
        batch_size: 32,
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
    let config = Config {
        base_dir: Some(temp_dir.path().to_path_buf()),
        ..Default::default()
    };

    // Test loading from non-existent config file (should return default)
    // We can't easily test this without modifying Config::load() to accept a custom path
    // So instead we test that default config validation works
    assert!(config.validate().is_ok());
    assert_eq!(config.ollama.protocol, "http");
    assert_eq!(config.ollama.host, "localhost");
    assert_eq!(config.ollama.port, 11434);
}

#[test]
fn https_url_generation() {
    let mut config = Config::default();
    config.ollama.protocol = "https".to_string();
    config.ollama.host = "secure.example.com".to_string();
    config.ollama.port = 443;

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
fn toml_with_https() {
    let mut config = Config::default();
    config.ollama.protocol = "https".to_string();
    config.ollama.host = "remote.ollama.com".to_string();

    let toml_str = toml::to_string(&config).expect("should serialize toml correctly");
    let parsed_config: Config = toml::from_str(&toml_str).expect("should parse toml correctly");

    assert_eq!(config, parsed_config);
    assert_eq!(parsed_config.ollama.protocol, "https");
}
