use std::env;
use std::path::PathBuf;

pub fn app_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Sextant")
}

pub fn env_key(name: &str) -> String {
    env::var(name).unwrap_or_default().trim().to_string()
}
