use campuslingo_domain::memory::{MemoryInput, ReviewInput, review};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

use crate::{Result, StorageError, begin_as};

#[derive(Debug, FromRow)]
struct AttemptRow {
    id: Uuid,
    tenant_id: Uuid,
    course_id: Uuid,
    challenge_id: Uuid,
    option_count: i16,
    selected_index: Option<i16>,
    not_expired: bool,
    correct_index: i16,
    explanation: String,
    response: Option<serde_json::Value>,
}

#[derive(Debug, FromRow)]
struct CardRow {
    stability: Option<f32>,
    difficulty: Option<f32>,
    reps: i32,
    last_review: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct AnswerResult {
    pub correct: bool,
    pub correct_index: u8,
    pub explanation: String,
    pub rating: u8,
    pub scheduled_days: u32,
    pub due_at: DateTime<Utc>,
    pub duplicate: bool,
}

/// Grade an attempt using server-owned answers, update FSRS-6, append exactly
/// one review/reward pair, and make same-answer retries idempotent.
pub async fn answer_attempt(
    pool: &sqlx::PgPool,
    actor: Uuid,
    attempt_id: Uuid,
    selected_index: u8,
) -> Result<AnswerResult> {
    let mut transaction = begin_as(pool, actor).await?;
    let attempt = sqlx::query_as::<_, AttemptRow>(
        "SELECT a.id, a.tenant_id, a.course_id, a.challenge_id,
                a.option_count, a.selected_index, a.response,
                (a.expires_at > clock_timestamp()) AS not_expired,
                ca.correct_index, ca.explanation
         FROM attempts a
         JOIN challenge_answers ca ON ca.challenge_id = a.challenge_id
         WHERE a.id = $1 AND a.user_id = $2 AND can_study_course(a.course_id)
         FOR UPDATE OF a",
    )
    .bind(attempt_id)
    .bind(actor)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or(StorageError::NotFound)?;

    if selected_index >= u8::try_from(attempt.option_count).unwrap_or(0) {
        return Err(StorageError::Invalid("selected option is out of range"));
    }

    if let Some(previous) = attempt.response.as_ref() {
        if attempt.selected_index == Some(i16::from(selected_index)) {
            let rating = u8::try_from(
                previous
                    .get("rating")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(3),
            )
            .unwrap_or(3);
            let due_at = sqlx::query_scalar::<_, DateTime<Utc>>(
                "SELECT due_at FROM fsrs_cards WHERE user_id = $1 AND challenge_id = $2",
            )
            .bind(actor)
            .bind(attempt.challenge_id)
            .fetch_optional(&mut *transaction)
            .await?
            .unwrap_or_else(Utc::now);
            transaction.commit().await?;
            return Ok(AnswerResult {
                correct: previous
                    .get("correct")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                correct_index: u8::try_from(attempt.correct_index).unwrap_or(0),
                explanation: attempt.explanation,
                rating,
                scheduled_days: previous
                    .get("scheduled_days")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .unwrap_or(0),
                due_at,
                duplicate: true,
            });
        }
        return Err(StorageError::Conflict);
    }

    if !attempt.not_expired {
        return Err(StorageError::Conflict);
    }

    let correct = i16::from(selected_index) == attempt.correct_index;
    let rating = if correct { 3 } else { 1 };
    let card = sqlx::query_as::<_, CardRow>(
        "SELECT stability, difficulty, reps, last_review
         FROM fsrs_cards WHERE user_id = $1 AND challenge_id = $2 FOR UPDATE",
    )
    .bind(actor)
    .bind(attempt.challenge_id)
    .fetch_optional(&mut *transaction)
    .await?;

    let elapsed_days = match card.as_ref().and_then(|value| value.last_review) {
        Some(last_review) => sqlx::query_scalar::<_, i32>(
            "SELECT GREATEST(0, floor(extract(epoch FROM (clock_timestamp() - $1)) / 86400))::int",
        )
        .bind(last_review)
        .fetch_one(&mut *transaction)
        .await?
        .max(0) as u32,
        None => 0,
    };

    let prior_memory = card
        .as_ref()
        .and_then(|value| match (value.stability, value.difficulty) {
            (Some(stability), Some(difficulty)) => Some(MemoryInput {
                stability,
                difficulty,
            }),
            _ => None,
        });
    let memory = review(&ReviewInput {
        memory: prior_memory,
        elapsed_days,
        rating,
        desired_retention: 0.9,
        parameters: None,
    })
    .map_err(|_| StorageError::Invalid("invalid memory state"))?;

    let state_before = serde_json::json!({
        "stability": card.as_ref().and_then(|value| value.stability),
        "difficulty": card.as_ref().and_then(|value| value.difficulty),
        "reps": card.as_ref().map_or(0, |value| value.reps),
        "last_review": card.as_ref().and_then(|value| value.last_review),
    });
    let state_after = serde_json::json!({
        "stability": memory.stability,
        "difficulty": memory.difficulty,
        "scheduled_days": memory.scheduled_days,
        "interval_days": memory.interval_days,
        "rating": rating,
    });
    let response = serde_json::json!({
        "selected_index": selected_index,
        "correct": correct,
        "rating": rating,
        "scheduled_days": memory.scheduled_days,
    });

    let updated = sqlx::query(
        "UPDATE attempts
         SET selected_index = $2, answered_at = clock_timestamp(), response = $3
         WHERE id = $1 AND user_id = $4 AND selected_index IS NULL",
    )
    .bind(attempt.id)
    .bind(i16::from(selected_index))
    .bind(&response)
    .bind(actor)
    .execute(&mut *transaction)
    .await?
    .rows_affected();
    if updated != 1 {
        return Err(StorageError::Conflict);
    }

    let review_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO review_logs
            (attempt_id, user_id, tenant_id, course_id, challenge_id, rating,
             state_before, state_after)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING id",
    )
    .bind(attempt.id)
    .bind(actor)
    .bind(attempt.tenant_id)
    .bind(attempt.course_id)
    .bind(attempt.challenge_id)
    .bind(i16::from(rating))
    .bind(&state_before)
    .bind(&state_after)
    .fetch_one(&mut *transaction)
    .await?;

    let due_at = sqlx::query_scalar::<_, DateTime<Utc>>(
        "INSERT INTO fsrs_cards
            (user_id, challenge_id, tenant_id, course_id, stability, difficulty,
             reps, last_review, due_at)
         VALUES ($1, $2, $3, $4, $5, $6, 1, clock_timestamp(),
                 clock_timestamp() + make_interval(days => $7))
         ON CONFLICT (user_id, challenge_id) DO UPDATE SET
             stability = EXCLUDED.stability,
             difficulty = EXCLUDED.difficulty,
             reps = fsrs_cards.reps + 1,
             last_review = clock_timestamp(),
             due_at = EXCLUDED.due_at,
             scheduler_version = EXCLUDED.scheduler_version
         RETURNING due_at",
    )
    .bind(actor)
    .bind(attempt.challenge_id)
    .bind(attempt.tenant_id)
    .bind(attempt.course_id)
    .bind(memory.stability)
    .bind(memory.difficulty)
    .bind(i32::try_from(memory.scheduled_days).unwrap_or(36_500))
    .fetch_one(&mut *transaction)
    .await?;

    sqlx::query(
        "INSERT INTO reward_ledger (review_id, user_id, tenant_id, course_id, amount)
         VALUES ($1, $2, $3, $4, 1)",
    )
    .bind(review_id)
    .bind(actor)
    .bind(attempt.tenant_id)
    .bind(attempt.course_id)
    .execute(&mut *transaction)
    .await?;

    transaction.commit().await?;
    Ok(AnswerResult {
        correct,
        correct_index: u8::try_from(attempt.correct_index).unwrap_or(0),
        explanation: attempt.explanation,
        rating,
        scheduled_days: memory.scheduled_days,
        due_at,
        duplicate: false,
    })
}
