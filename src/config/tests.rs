use super::*;
use crate::embeddings::chunking::ChunkingConfig;
use tempfile::TempDir;

#[cfg(test)]
mod integration_tests {
    use crate::embeddings::ollama::DEFAULT_EMBEDDING_DIMENSION;

    use super::*;

    #[test]
    fn complete_valid_config() {
        let valid_toml = r#"
            [ollama]
            protocol = "http"
            host = "localhost"
            port = 11434
            model = "nomic-embed-text:latest"
            batch_size = 64
        "#;

        let config: Config = toml::from_str(valid_toml).expect("should parse toml successfully");
        assert_eq!(config.ollama.protocol, "http");
        assert_eq!(config.ollama.host, "localhost");
        assert_eq!(config.ollama.port, 11434);
        assert_eq!(config.ollama.model, "nomic-embed-text:latest");
        assert_eq!(config.ollama.batch_size, 64);
    }

    #[test]
    fn config_validation_edge_cases() {
        let temp_dir = TempDir::new().expect("should create TempDir successfully");
        let config = Config {
            ollama: OllamaConfig {
                protocol: "http".to_string(),
                host: String::new(),
                port: 80,
                model: "test".to_string(),
                batch_size: 1,
                embedding_dimension: DEFAULT_EMBEDDING_DIMENSION,
            },
            chunking: ChunkingConfig::default(),
            base_dir: temp_dir.path().to_path_buf(),
        };

        let result = config.validate();
        assert!(result.is_err()); // Empty host should be invalid
    }

    #[test]
    fn port_boundary_validation() {
        let mut config = OllamaConfig {
            protocol: "http".to_string(),
            host: "localhost".to_string(),
            port: 1,
            model: "test".to_string(),
            batch_size: 1,
            embedding_dimension: DEFAULT_EMBEDDING_DIMENSION,
        };

        assert!(config.set_port(1).is_ok());
        assert!(config.set_port(65535).is_ok());
        assert!(config.set_port(0).is_err());
    }

    #[test]
    fn batch_size_boundary_validation() {
        let mut config = OllamaConfig {
            protocol: "http".to_string(),
            host: "localhost".to_string(),
            port: 11434,
            model: "test".to_string(),
            batch_size: 1,
            embedding_dimension: DEFAULT_EMBEDDING_DIMENSION,
        };

        assert!(config.set_batch_size(1).is_ok());
        assert!(config.set_batch_size(1000).is_ok());
        assert!(config.set_batch_size(0).is_err());
        assert!(config.set_batch_size(1001).is_err());
    }

    #[test]
    fn ollama_url_generation_with_different_hosts() {
        let configs = vec![
            ("http", "localhost", 11434, "http://localhost:11434/"),
            ("http", "127.0.0.1", 8080, "http://127.0.0.1:8080/"),
            ("http", "example.com", 3000, "http://example.com:3000/"),
            (
                "https",
                "secure.example.com",
                443,
                "https://secure.example.com/",
            ),
        ];
        let temp_dir = TempDir::new().expect("should create TempDir successfully");

        for (protocol, host, port, expected_url) in configs {
            let config = Config {
                ollama: OllamaConfig {
                    protocol: protocol.to_string(),
                    host: host.to_string(),
                    port,
                    model: "test".to_string(),
                    batch_size: 32,
                    embedding_dimension: DEFAULT_EMBEDDING_DIMENSION,
                },
                chunking: ChunkingConfig::default(),
                base_dir: temp_dir.path().to_path_buf(),
            };

            let url = config.ollama.ollama_url().expect("ollama_url is ok");
            assert_eq!(url.as_str(), expected_url);
        }
    }

    #[test]
    fn model_name_validation() {
        let mut config = OllamaConfig {
            protocol: "http".to_string(),
            host: "localhost".to_string(),
            port: 11434,
            model: "valid-model".to_string(),
            batch_size: 32,
            embedding_dimension: DEFAULT_EMBEDDING_DIMENSION,
        };

        assert!(config.set_model("valid-model".to_string()).is_ok());
        assert!(config.set_model("another_model".to_string()).is_ok());
        assert!(config.set_model(String::new()).is_err());
        assert!(config.set_model("   ".to_string()).is_err()); // Only whitespace
    }

    #[test]
    fn error_display_messages() {
        let errors = vec![
            ConfigError::InvalidProtocol("ftp".to_string()),
            ConfigError::InvalidPort(0),
            ConfigError::InvalidBatchSize(0),
            ConfigError::InvalidModel(String::new()),
            ConfigError::InvalidUrl("invalid-url".to_string()),
        ];

        for error in errors {
            let message = format!("{error}");
            assert!(!message.is_empty());
            assert!(message.len() > 10); // Ensure meaningful error messages
        }
    }
}
