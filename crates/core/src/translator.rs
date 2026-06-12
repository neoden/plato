//! Online translation providers.

use std::time::Duration;
use anyhow::{Error, format_err};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde_json::Value as JsonValue;
use crate::settings::TranslationProvider;

// Stay well below typical URL length limits.
const QUERY_LENGTH_LIMIT: usize = 4096;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

pub struct Translation {
    pub text: String,
    pub detected_source: Option<String>,
}

pub fn translate(query: &str, source: &str, target: &str, provider: TranslationProvider) -> Result<Translation, Error> {
    match provider {
        TranslationProvider::Google => translate_google(query, source, target),
    }
}

// Unofficial endpoint used by the Google Translate browser extension
// (and KOReader): no API key, plain GET, JSON array response.
fn translate_google(query: &str, source: &str, target: &str) -> Result<Translation, Error> {
    let mut query = query;
    while query.len() > QUERY_LENGTH_LIMIT {
        query = &query[..query.char_indices().rev()
                              .find(|(i, _)| *i <= QUERY_LENGTH_LIMIT)
                              .map(|(i, _)| i).unwrap_or(0)];
    }

    let url = format!("https://translate.googleapis.com/translate_a/single?client=gtx&sl={}&tl={}&dt=t&q={}",
                      source, target, utf8_percent_encode(query, NON_ALPHANUMERIC));

    let agent: ureq::Agent = ureq::Agent::config_builder()
                                  .timeout_global(Some(REQUEST_TIMEOUT))
                                  .build().into();
    let body = agent.get(&url).call()?
                    .body_mut().read_to_string()?;
    let value: JsonValue = serde_json::from_str(&body)?;

    let segments = value.get(0).and_then(JsonValue::as_array)
                        .ok_or_else(|| format_err!("Unexpected response: {}", &body[..body.len().min(128)]))?;
    let mut text = String::new();
    for segment in segments {
        if let Some(s) = segment.get(0).and_then(JsonValue::as_str) {
            text.push_str(s);
        }
    }

    let detected_source = value.get(2).and_then(JsonValue::as_str)
                               .map(String::from);

    Ok(Translation { text, detected_source })
}

// Network errors are worth retrying while the wifi connection
// is being established; anything else isn't.
pub fn is_transient(err: &Error) -> bool {
    matches!(err.downcast_ref::<ureq::Error>(),
             Some(ureq::Error::Io(..)) |
             Some(ureq::Error::Timeout(..)) |
             Some(ureq::Error::ConnectionFailed) |
             Some(ureq::Error::HostNotFound))
}

// Map a book's metadata language (e.g. `English`, `en-US`, `ru`)
// to a provider language code.
pub fn normalize_language(language: &str) -> Option<String> {
    let language = language.trim();
    let code = language.split(|c: char| c == '-' || c == '_').next()?;
    if code.len() == 2 && code.chars().all(|c| c.is_ascii_alphabetic()) {
        Some(code.to_lowercase())
    } else {
        None
    }
}
