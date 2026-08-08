pub mod chatgpt;
pub mod gemini;

use crate::provider::{Provider, ProviderError};

/// Map model/provider name to a concrete provider.
/// Accepts both canonical names and OpenAI-style model aliases.
pub fn create_provider(name: &str) -> Result<Box<dyn Provider>, ProviderError> {
    match name {
        // ChatGPT — canonical + OpenAI model aliases
        "chatgpt" | "gpt-4o" | "gpt-4" | "gpt-3.5-turbo" => {
            Ok(Box::new(chatgpt::ChatGptProvider::new()))
        }
        // Gemini
        "gemini" | "gemini-pro" | "gemini-flash" => {
            Ok(Box::new(gemini::GeminiProvider::new()))
        }
        _ => Err(ProviderError::NotAvailable(name.to_string())),
    }
}

pub fn list_providers() -> Vec<&'static str> {
    vec!["chatgpt", "gemini"]
}
