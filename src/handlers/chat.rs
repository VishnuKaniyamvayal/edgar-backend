use axum::{
    Json, extract::State, http::StatusCode, response::{
        IntoResponse, sse::{Event, KeepAlive, Sse}
    }
};
use bytes::Bytes;
use futures::stream::Stream;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;
use std::{convert::Infallible, sync::Arc};

use crate::{
    prompt::{
        SearchResult, build_direct_prompt, build_rewrite_prompt,
        build_search_decision_prompt, build_search_prompt,
    },
    repository,
    state::AppState,
};

// ─── Request / Response Types ────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ChatRequest {
    pub message: String,
    pub conversation_id: Option<Uuid>
}

#[derive(Serialize)]
struct TavilyRequest {
    query: String,
    search_depth: String,
}

type ChatError = (StatusCode, String);

// ─── Handler ─────────────────────────────────────────────────────────────────

pub async fn chat_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ChatRequest>,
) -> impl IntoResponse {
    let request_id = Uuid::new_v4();
    println!(
        "[chat:{}] start message_len={} conversation_id={:?}",
        request_id,
        payload.message.len(),
        payload.conversation_id
    );

    let tavily_key = dotenvy::var("TAVILY_API_KEY").unwrap_or_default();
    let gemini_key = dotenvy::var("GEMINI_API_KEY").unwrap_or_default();
    let client = Client::new();
    println!(
        "[chat:{}] keys tavily_present={} gemini_present={}",
        request_id,
        !tavily_key.is_empty(),
        !gemini_key.is_empty()
    );

    // 1. Resolve or create conversation
    let conversation_id = match payload.conversation_id {
        Some(id) => {
            println!("[chat:{}] using existing conversation_id={}", request_id, id);
            id
        }
        None => repository::create_conversation(&state.db)
            .await
            .map_err(|e| {
                eprintln!("[chat:{}] create_conversation failed: {}", request_id, e);
                (StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {}", e))
            })?,
    };
    println!(
        "[chat:{}] conversation resolved id={}",
        request_id, conversation_id
    );

    // 2. Fetch history
    let history = repository::get_conversation_history(&state.db, conversation_id, 10)
        .await
        .unwrap_or_default();
    println!(
        "[chat:{}] history loaded count={}",
        request_id,
        history.len()
    );

    // 3. Save user message
    repository::save_message(&state.db, conversation_id, "user", &payload.message)
        .await
        .map_err(|e| {
            eprintln!("[chat:{}] save user message failed: {}", request_id, e);
            (StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {}", e))
        })?;
    println!("[chat:{}] user message saved", request_id);

    // 4. Decide if search is needed
    let decision_prompt = build_search_decision_prompt(&history, &payload.message);
    println!(
        "[chat:{}] calling decision model prompt_len={}",
        request_id,
        decision_prompt.len()
    );
    let decision = call_gemini_once(&client, &gemini_key, &decision_prompt).await?;
    let needs_search = decision.to_uppercase().contains("YES");
    println!(
        "[chat:{}] decision='{}' needs_search={}",
        request_id,
        decision,
        needs_search
    );

    // 5. Build prompt + results based on decision
    let (prompt, results) = if needs_search {
        // Rewrite query if this is a follow-up, otherwise use message as-is
        let search_query = if history.is_empty() {
            println!("[chat:{}] no history, using message as search query", request_id);
            payload.message.clone()
        } else {
            println!("[chat:{}] rewriting follow-up query", request_id);
            let rewrite_prompt = build_rewrite_prompt(&history, &payload.message);
            call_gemini_once(&client, &gemini_key, &rewrite_prompt).await?
        };
        println!(
            "[chat:{}] searching tavily query_len={}",
            request_id,
            search_query.len()
        );
        let results = search_tavily(&client, &tavily_key, &search_query).await?;
        println!("[chat:{}] tavily results count={}", request_id, results.len());
        let prompt = build_search_prompt(&payload.message, &results);
        println!(
            "[chat:{}] built search prompt_len={}",
            request_id,
            prompt.len()
        );
        (prompt, results)
    } else {
        let prompt = build_direct_prompt(&payload.message, &history);
        println!(
            "[chat:{}] built direct prompt_len={}",
            request_id,
            prompt.len()
        );
        (prompt, vec![])
    };

    // 6. Stream Gemini response back to client
    println!("[chat:{}] calling gemini streaming endpoint", request_id);
    let response = call_gemini_stream(&client, &gemini_key, &prompt).await?;
    let sources_event = build_sources_event(&results, conversation_id);
    let token_stream = make_sse_stream(response.bytes_stream());
    let saving_stream = make_saving_stream(token_stream, state.db.clone(), conversation_id);
    println!("[chat:{}] streaming response started", request_id);

    let full_stream = futures::stream::once(async move { Ok(sources_event) })
        .chain(saving_stream);

    Ok::<_, ChatError>(Sse::new(full_stream).keep_alive(KeepAlive::default()))
}


// ─── Tavily Search ───────────────────────────────────────────────────────────

async fn search_tavily(
    client: &Client,
    api_key: &str,
    query: &str,
) -> Result<Vec<SearchResult>, ChatError> {
    println!(
        "[search_tavily] start query='{}'",
        query.chars().take(120).collect::<String>()
    );
    let response = client
        .post("https://api.tavily.com/search")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&TavilyRequest {
            query: query.to_string(),
            search_depth: "advanced".to_string(),
        })
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Tavily request failed: {}", e)))?;

    println!("[search_tavily] http_status={}", response.status());

    let json: Value = response
        .json()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Failed to parse Tavily response: {}", e)))?;

    let results: Vec<SearchResult> = json["results"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|r| SearchResult {
            title: r["title"].as_str().unwrap_or("").to_string(),
            url: r["url"].as_str().unwrap_or("").to_string(),
            content: r["content"].as_str().unwrap_or("").to_string(),
        })
        .collect();

    println!("[search_tavily] parsed results_count={}", results.len());

    Ok(results)
}

// ─── Gemini Streaming Call ───────────────────────────────────────────────────

async fn call_gemini_once(
    client: &Client,
    api_key: &str,
    prompt: &str,
) -> Result<String, ChatError> {
    println!("[call_gemini_once] prompt_len={}", prompt.len());
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.0-flash:generateContent?key={}",
        api_key
    );
    let body = json!({ "contents": [{ "parts": [{ "text": prompt }] }] });

    let json: Value = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?
        .json()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;

    println!("[call_gemini_once] response received");

    Ok(json["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string())
}

async fn call_gemini_stream(
    client: &Client,
    api_key: &str,
    prompt: &str,
) -> Result<reqwest::Response, ChatError> {
    println!("[call_gemini_stream] prompt_len={}", prompt.len());
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-flash-latest:streamGenerateContent?alt=sse&key={}",
        api_key
    );

    let body = json!({
        "contents": [
            {
                "parts": [
                    { "text": prompt }
                ]
            }
        ]
    });

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "text/event-stream")
        .json(&body)
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Gemini request failed: {}", e)))?;

    println!("[call_gemini_stream] http_status={}", response.status());
    Ok(response)
}

// ─── Sources Event ───────────────────────────────────────────────────────────

fn build_sources_event(results: &[SearchResult], conversation_id: Uuid) -> Event {
    let sources_json = serde_json::to_string(&json!({
        "conversation_id": conversation_id,
        "sources": results
            .iter()
            .enumerate()
            .map(|(i, r)| json!({ "index": i + 1, "title": r.title, "url": r.url }))
            .collect::<Vec<_>>()
    }))
    .unwrap_or_default();

    Event::default().event("sources").data(sources_json)
}

// ─── Saving Stream Wrapper ───────────────────────────────────────────────────

fn make_saving_stream(
    token_stream: impl Stream<Item = Result<String, Infallible>> + Send + 'static,
    db_pool: sqlx::PgPool,
    conversation_id: Uuid,
) -> impl Stream<Item = Result<Event, Infallible>> + Send + 'static {
    println!(
        "[make_saving_stream] collector started conversation_id={}",
        conversation_id
    );
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    tokio::spawn(async move {
        let mut full_response = String::new();
        while let Some(token) = rx.recv().await {
            full_response.push_str(&token);
        }
        if !full_response.is_empty() {
            println!(
                "[make_saving_stream] saving assistant message len={} conversation_id={}",
                full_response.len(),
                conversation_id
            );
            if let Err(e) = repository::save_message(
                &db_pool, conversation_id, "assistant", &full_response,
            )
            .await
            {
                eprintln!("[make_saving_stream] save assistant message failed: {}", e);
            } else {
                println!("[make_saving_stream] assistant message saved");
            }
        }
    });

    token_stream.map(move |result| match result {
        Ok(token) => {
            let _ = tx.send(token.clone());
            Ok(Event::default().data(token))
        }
        Err(err) => Err(err),
    })
}

// ─── SSE Stream Parser ──────────────────────────────────────────────────────

fn make_sse_stream(
    byte_stream: impl Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static,
) -> impl Stream<Item = Result<String, Infallible>> + Send + 'static {
    let buffer = String::new();

    futures::stream::unfold(
        (Box::pin(byte_stream), buffer),
        |(mut stream, mut buf)| async move {
            loop {
                if let Some(line_end) = buf.find('\n') {
                    let line = buf[..line_end].to_string();
                    buf = buf[line_end + 1..].to_string();

                    if let Some(data) = line.strip_prefix("data: ") {
                        if let Some(event) = parse_gemini_token(data) {
                            return Some((Ok(event), (stream, buf)));
                        }
                    }
                    continue;
                }

                match stream.next().await {
                    Some(Ok(bytes)) => {
                        buf.push_str(&String::from_utf8_lossy(&bytes));
                    }
                    Some(Err(_)) => return None,
                    None => {
                        if !buf.is_empty() {
                            if let Some(data) = buf.strip_prefix("data: ") {
                                if let Some(event) = parse_gemini_token(data) {
                                    buf.clear();
                                    return Some((Ok(event), (stream, buf)));
                                }
                            }
                        }
                        return None;
                    }
                }
            }
        },
    )
}

// ─── Token Extraction ────────────────────────────────────────────────────────

fn parse_gemini_token(data: &str) -> Option<String> {
    let parsed: Value = serde_json::from_str(data).ok()?;
    let text = parsed["candidates"][0]["content"]["parts"][0]["text"].as_str()?;

    if text.is_empty() {
        return None;
    }

    Some(text.to_string())
}