use sqlx::PgPool;
use uuid::Uuid;

use crate::models::Message;

pub async fn create_conversation(pool: &PgPool) -> Result<Uuid, sqlx::Error> {
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO conversations DEFAULT VALUES RETURNING id"
    )
    .persistent(false)
    .fetch_one(pool)
    .await?;

    Ok(row.0)
}

pub async fn get_conversation_history(
    pool: &PgPool,
    conversation_id: Uuid,
    limit: i64,
) -> Result<Vec<Message>, sqlx::Error> {
    let messages = sqlx::query_as::<_, Message>(
        "SELECT id, conversation_id, role, content, sources, created_at
         FROM messages
         WHERE conversation_id = $1
         ORDER BY created_at ASC
         LIMIT $2"
    )
    .persistent(false)
    .bind(conversation_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(messages)
}

pub async fn save_message(
    pool: &PgPool,
    conversation_id: Uuid,
    role: &str,
    content: &str,
) -> Result<Uuid, sqlx::Error> {
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO messages (conversation_id, role, content)
         VALUES ($1, $2, $3)
         RETURNING id"
    )
    .persistent(false)
    .bind(conversation_id)
    .bind(role)
    .bind(content)
    .fetch_one(pool)
    .await?;

    Ok(row.0)
}