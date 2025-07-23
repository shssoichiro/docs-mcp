use super::load_existing_config as load_existing_config_impl;

#[test]
fn load_existing_config() {
    let config = load_existing_config_impl().expect("config loaded successfully");
    assert!(!config.ollama.host.is_empty());
    assert!(config.ollama.port > 0);
    assert!(!config.ollama.model.is_empty());
    assert!(config.ollama.batch_size > 0);
}
