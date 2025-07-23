use super::*;

#[test]
fn default_config() {
    let config = Config::default();
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
        host: "localhost".to_string(),
        port: 11434,
        model: "test-model".to_string(),
        batch_size: 32,
    };

    assert!(config.set_host("example.com".to_string()).is_ok());
    assert!(config.set_port(8080).is_ok());
    assert!(config.set_model("new-model".to_string()).is_ok());
    assert!(config.set_batch_size(128).is_ok());

    assert!(config.set_port(0).is_err());
    assert!(config.set_model(String::new()).is_err());
    assert!(config.set_batch_size(0).is_err());
    assert!(config.set_batch_size(1001).is_err());
}

#[test]
fn load_missing_config() {
    let config = Config::load().expect("should load config successfully");
    assert_eq!(config, Config::default());
}
