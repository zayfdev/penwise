//! Handwriting → text through a local vision model (Ollama), with the result cached beside the page.
//!
//! Nothing leaves the machine: the page PNG is posted to `PENWISE_OCR_URL` (default `http://127.0.0.1:11434`),
//! which is expected to be a local Ollama. When no model is reachable the page simply has no text — the ink and
//! the rest of the surface keep working.
//!
//! Plain HTTP only (the client is built without TLS): the endpoint is a local model, never a remote service.

use std::time::Duration;

use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};

use crate::AppState;
use crate::{AuthQuery, auth, render, resolve_page};

/// `GET /text?page=<id>&refresh=1` — the page's handwriting as text, cached beside the page.
/// `GET /text?page=<id>&refresh=1` — the page's handwriting as text, cached beside the page.
pub async fn page_text(
    State(app): State<AppState>,
    Query(query): Query<TextQuery>,
    Query(auth_query): Query<AuthQuery>,
    headers: axum::http::HeaderMap,
) -> Response {
    if !auth::authorized(&app.token, auth_query.token.as_deref(), &headers) {
        return auth::deny();
    }
    let page = match resolve_page(&app, query.page.as_deref()).await {
        Ok(page) => page,
        Err(response) => return response,
    };
    if !query.refresh.unwrap_or(false)
        && let Some(text) = app.library.text(&page.id).await
    {
        return axum::Json(serde_json::json!({
            "page": page.id,
            "text": text,
            "cached": true,
        }))
        .into_response();
    }
    let strokes = app.library.strokes(&page.id).await;
    if strokes.is_empty() {
        return axum::Json(serde_json::json!({
            "page": page.id,
            "text": "",
            "cached": false,
        }))
        .into_response();
    }
    let scale = read_scale(query.scale);
    let png = match render::ink_png(&strokes, scale, OCR_PADDING_PX) {
        Ok(Some(png)) => png,
        Ok(None) => {
            return axum::Json(serde_json::json!({
                "page": page.id,
                "text": "",
                "cached": false,
            }))
            .into_response();
        }
        Err(error) => {
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, error).into_response();
        }
    };
    match transcribe(png).await {
        Ok(text) => {
            app.library.put_text(&page.id, &text).await;
            axum::Json(serde_json::json!({
                "page": page.id,
                "text": text,
                "cached": false,
            }))
            .into_response()
        }
        Err(error) => (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            format!("handwriting model unavailable: {error}"),
        )
            .into_response(),
    }
}

/// Query of the text route.
#[derive(serde::Deserialize)]
pub struct TextQuery {
    pub page: Option<String>,
    pub refresh: Option<bool>,
    pub scale: Option<f32>,
}

const DEFAULT_URL: &str = "http://127.0.0.1:11434";
const DEFAULT_MODEL: &str = "qwen2.5vl:3b";
/// Margin around the ink when it is cropped for recognition, in page px.
const OCR_PADDING_PX: f32 = 48.0;
/// A page render can take a while on CPU; the caller is an agent tool, not the pen path.
const TIMEOUT_SECS: u64 = 300;
const MIN_SCALE: f32 = 0.5;
const MAX_SCALE: f32 = 2.0;

/// Renders `scale` to use: pages are read at a scale that keeps small handwriting legible without huge images.
#[must_use]
pub fn read_scale(requested: Option<f32>) -> f32 {
    requested
        .filter(|scale| scale.is_finite())
        .map_or(1.0, |scale| scale.clamp(MIN_SCALE, MAX_SCALE))
}

/// The prompt: a transcription, not a description, so agents get text they can search.
const PROMPT: &str = "Transcribe the handwriting in this image exactly as written. \
Output only the text, preserving line breaks and bullet points. \
Do not describe the image, do not add commentary, do not correct spelling. \
If the page is blank, output nothing.";

/// Asks the local model to transcribe `png`.
pub async fn transcribe(png: Vec<u8>) -> Result<String, String> {
    let url = std::env::var("PENWISE_OCR_URL").unwrap_or_else(|_| DEFAULT_URL.to_owned());
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
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard_alphabet() {
        assert_eq!("", base64(b""));
        assert_eq!("Zg==", base64(b"f"));
        assert_eq!("Zm8=", base64(b"fo"));
        assert_eq!("Zm9v", base64(b"foo"));
        assert_eq!("Zm9vYg==", base64(b"foob"));
        assert_eq!("Zm9vYmE=", base64(b"fooba"));
        assert_eq!("Zm9vYmFy", base64(b"foobar"));
    }

    #[test]
    fn model_chatter_is_trimmed_but_line_breaks_stay() {
        assert_eq!("milk\neggs", tidy("  milk\neggs \n"));
        assert_eq!("milk\neggs", tidy("```\nmilk\neggs\n```"));
        assert_eq!("milk", tidy("```text\nmilk```"));
        assert_eq!("", tidy("   "));
    }

    #[test]
    fn read_scale_is_bounded() {
        assert!((1.0 - read_scale(None)).abs() < f32::EPSILON);
        assert!((0.5 - read_scale(Some(0.01))).abs() < f32::EPSILON);
        assert!((2.0 - read_scale(Some(9.0))).abs() < f32::EPSILON);
        assert!((1.0 - read_scale(Some(f32::NAN))).abs() < f32::EPSILON);
    }

    #[test]
    fn truncation_respects_character_boundaries() {
        assert_eq!("héllo", truncate("héllo wörld", 6));
        assert_eq!("", truncate("anything", 0));
    }
}
