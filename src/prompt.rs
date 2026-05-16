pub fn build_search_prompt(query: &str, results: &[SearchResult]) -> String {
    let mut context = String::new();

    for (i, result) in results.iter().enumerate() {
        context.push_str(&format!(
            "[{}] Title: {}\nURL: {}\nContent: {}\n\n",
            i + 1,
            result.title,
            result.url,
            result.content
        ));
    }

    format!(
        r#"You are Edgar, a helpful research assistant. Answer the user's question based ONLY on the provided search results. Be concise, accurate, and informative.

Rules:
- Cite sources using [number] notation inline (e.g. [1], [2]).
- If multiple sources support a point, cite all relevant ones (e.g. [1][3]).
- Do NOT make up information not present in the sources.
- If the sources don't contain enough information to fully answer, say so.
- Use markdown formatting for readability.
- Keep the answer focused and well-structured.

Search Results:
{context}

User Question: {query}

Answer:"#,
        context = context,
        query = query
    )
}

pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub content: String,
}

use crate::models::Message;

pub fn build_rewrite_prompt(history: &[Message], follow_up: &str) -> String {
    let mut conversation = String::new();

    for msg in history {
        let role = if msg.role == "user" { "User" } else { "Assistant" };
        conversation.push_str(&format!("{}: {}\n", role, msg.content));
    }

    format!(
        r#"Given the following conversation history and a follow-up message, rewrite the follow-up as a standalone search query that captures the full intent.

Rules:
- Output ONLY the rewritten query, nothing else.
- Make it specific enough to get good web search results.
- Include relevant context from the conversation that the follow-up refers to.
- If the follow-up is already a standalone question, return it as-is.

Conversation:
{conversation}

Follow-up: {follow_up}

Rewritten query:"#,
        conversation = conversation,
        follow_up = follow_up
    )
}

pub fn build_direct_prompt(query: &str, history: &[Message]) -> String {
    let mut conversation = String::new();

    for msg in history {
        let role = if msg.role == "user" { "User" } else { "Assistant" };
        conversation.push_str(&format!("{}: {}\n", role, msg.content));
    }

    format!(
        r#"You are Edgar, a helpful assistant. Answer the user's message using the conversation history as context. Be concise and helpful.

Rules:
- Use markdown formatting for readability.
- If the message is conversational (e.g. "how are you"), respond naturally.
- Do not make up facts — if you don't know something, say so.

Conversation History:
{conversation}

User: {query}

Answer:"#,
        conversation = conversation,
        query = query
    )
}

pub fn build_search_decision_prompt(history: &[Message], follow_up: &str) -> String {
    let mut conversation = String::new();

    for msg in history {
        let role = if msg.role == "user" { "User" } else { "Assistant" };
        conversation.push_str(&format!("{}: {}\n", role, msg.content));
    }

    format!(
        r#"You are deciding whether a web search is needed to answer a follow-up message in a conversation.

Analyze the conversation history and the follow-up message. Decide if the existing conversation already contains enough information to answer it, or if a fresh web search is needed.

Search IS needed when:
- The follow-up asks about new facts, people, places, or events not covered in the history
- The follow-up asks for current/live data (prices, news, scores, weather)
- The follow-up shifts to a completely different topic
- The history only partially covers the question and more detail is needed

Search is NOT needed when:
- The message is conversational or social (e.g. "how are you", "thanks", "hello", "got it", "ok")
- The follow-up is asking for clarification or re-explanation of something already in the history
- The follow-up is a simple follow-on like "summarize that", "explain it simply", "give me an example"
- All the facts needed to answer are already present in the assistant's previous responses
- The question is about something the assistant can answer from general knowledge without needing live data (e.g. "what does API stand for", "explain recursion")
- The message is an opinion question or chit-chat that doesn't need factual web data

Conversation History:
{conversation}

Follow-up: {follow_up}

Reply with ONLY one word: YES (search needed) or NO (history is sufficient)."#,
        conversation = conversation,
        follow_up = follow_up
    )
}