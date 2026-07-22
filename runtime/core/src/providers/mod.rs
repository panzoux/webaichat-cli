pub mod chatgpt;
pub mod gemini;

use crate::provider::{Provider, ProviderError};

pub fn create_provider(name: &str) -> Result<Box<dyn Provider>, ProviderError> {
    match name {
        "chatgpt" => Ok(Box::new(chatgpt::ChatGptProvider::new())),
        "gemini" => Ok(Box::new(gemini::GeminiProvider::new())),
        _ => Err(ProviderError::NotAvailable(name.to_string())),
    }
}

pub fn list_providers() -> Vec<&'static str> {
    vec!["chatgpt", "gemini"]
}
