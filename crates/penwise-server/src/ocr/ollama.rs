//! The Ollama client: one `POST /api/generate` with the cropped ink and a transcription prompt.
//!
//! Plain HTTP only (the client is built without TLS): the endpoint is a local model, never a remote service.

use std::time::Duration;

const DEFAULT_MODEL: &str = "qwen2.5vl:3b";
/// A page render can take a while on CPU; the caller is an agent tool, not the pen path.
const TIMEOUT_SECS: u64 = 300;

/// The prompt: a transcription, not a description, so agents get text they can search.
const PROMPT: &str = "Transcribe the handwriting in this image exactly as written. \
Output only the text, preserving line breaks and bullet points. \
Do not describe the image, do not add commentary, do not correct spelling. \
If the page is blank, output nothing.";

/// Asks the local model at `url` to transcribe `png`.
pub(super) async fn transcribe(url: &str, png: Vec<u8>) -> Result<String, String> {
    let model = std::env::var("PENWISE_OCR_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_owned());
    let body = serde_json::json!({
        "model": model,
        "prompt": PROMPT,
        "images": [base64(&png)],
        "stream": false,
        "options": { "temperature": 0.0 },
    });
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(TIMEOUT_SECS))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .post(format!("{}/api/generate", url.trim_end_matches('/')))
        .json(&body)
        .send()
        .await
        .map_err(|error| format!("ocr request failed: {error}"))?;
    let status = response.status();
    let text = response.text().await.map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!("ocr replied {status}: {}", truncate(&text, 200)));
    }
    let parsed: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| format!("ocr reply is not JSON: {error}"))?;
    let answer = parsed
        .get("response")
        .and_then(serde_json::Value::as_str)
        .ok_or("ocr reply has no response")?;
    Ok(tidy(answer))
}

/// Trims model chatter and trailing whitespace, keeping the transcription's own line breaks.
fn tidy(answer: &str) -> String {
    let cleaned = answer.trim();
    let cleaned = cleaned
        .strip_prefix("```")
        .and_then(|rest| rest.split_once('\n'))
        .map_or(cleaned, |(_, body)| body.trim_end_matches("```").trim());
    cleaned.trim_end().to_owned()
}

fn truncate(text: &str, limit: usize) -> &str {
    let mut end = 0;
    for (index, character) in text.char_indices() {
        if index >= limit {
            break;
        }
        end = index + character.len_utf8();
    }
    &text[..end]
}

/// Standard base64 (RFC 4648, with padding) for the image field.
fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        out.push(char::from(ALPHABET[(b[0] >> 2) as usize]));
        out.push(char::from(
            ALPHABET[(((b[0] & 0x03) << 4) | (b[1] >> 4)) as usize],
        ));
        if chunk.len() > 1 {
            out.push(char::from(
                ALPHABET[(((b[1] & 0x0F) << 2) | (b[2] >> 6)) as usize],
            ));
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(char::from(ALPHABET[(b[2] & 0x3F) as usize]));
        } else {
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
mod tests;
