use serde::Serialize;
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::{Result, StorageError, begin_as, is_permission_denied, is_unique_violation};

#[derive(Debug, Serialize, FromRow)]
pub struct TenantSummary {
    pub id: Uuid,
    pub name: String,
    pub owner: bool,
}

#[derive(Debug, Serialize, FromRow)]
pub struct CourseSummary {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub visibility: String,
    pub published: bool,
    pub can_manage: bool,
    pub can_study: bool,
}

#[derive(Debug, Serialize, FromRow)]
pub struct NodeSummary {
    pub id: Uuid,
    pub title: String,
}

#[derive(Debug, Serialize, FromRow)]
pub struct ChallengeSummary {
    pub id: Uuid,
    pub node_id: Uuid,
    pub prompt: String,
    pub options: Value,
}

#[derive(Debug, Serialize, FromRow)]
pub struct PracticeQuestion {
    pub id: Uuid,
    pub prompt: String,
    pub options: Value,
}

#[derive(Debug)]
pub struct CreatedAttempt {
    pub id: Uuid,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

pub async fn create_tenant(pool: &PgPool, actor: Uuid, name: &str) -> Result<Uuid> {
    let mut transaction = begin_as(pool, actor).await?;
    let tenant_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO tenants (owner_id, name) VALUES ($1, $2) RETURNING id",
    )
    .bind(actor)
    .bind(name)
    .fetch_one(&mut *transaction)
    .await?;
    sqlx::query("INSERT INTO tenant_members (tenant_id, user_id, role) VALUES ($1, $2, 'owner')")
        .bind(tenant_id)
        .bind(actor)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(tenant_id)
}

pub async fn list_tenants(pool: &PgPool, actor: Uuid) -> Result<Vec<TenantSummary>> {
    let mut transaction = begin_as(pool, actor).await?;
    let tenants = sqlx::query_as::<_, TenantSummary>(
        "SELECT id, name, (owner_id = $1) AS owner FROM tenants ORDER BY created_at, id",
    )
    .bind(actor)
    .fetch_all(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(tenants)
}

pub async fn add_tenant_member(
    pool: &PgPool,
    actor: Uuid,
    tenant_id: Uuid,
    user_id: Uuid,
    role: &str,
) -> Result<()> {
    if !matches!(role, "instructor" | "student") {
        return Err(StorageError::Invalid("role must be instructor or student"));
    }
    let mut transaction = begin_as(pool, actor).await?;
    sqlx::query(
        "INSERT INTO tenant_members (tenant_id, user_id, role)
         VALUES ($1, $2, $3)
         ON CONFLICT (tenant_id, user_id) DO UPDATE SET role = EXCLUDED.role, active = TRUE",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(role)
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        if is_permission_denied(&error) {
            StorageError::Forbidden
        } else if is_unique_violation(&error) {
            StorageError::Conflict
        } else {
            StorageError::Database(error)
        }
    })?;
    transaction.commit().await?;
    Ok(())
}

pub async fn create_course(
    pool: &PgPool,
    actor: Uuid,
    tenant_id: Uuid,
    name: &str,
) -> Result<Uuid> {
    let mut transaction = begin_as(pool, actor).await?;
    let course_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO courses (tenant_id, created_by, name)
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(actor)
    .bind(name)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|error| {
        if is_permission_denied(&error) {
            StorageError::Forbidden
        } else {
            StorageError::Database(error)
        }
    })?;
    transaction.commit().await?;
    Ok(course_id)
}

pub async fn list_courses(pool: &PgPool, actor: Uuid) -> Result<Vec<CourseSummary>> {
    let mut transaction = begin_as(pool, actor).await?;
    let courses = sqlx::query_as::<_, CourseSummary>(
        "SELECT id, tenant_id, name, visibility, published,
                can_manage_course(id) AS can_manage, can_study_course(id) AS can_study
         FROM courses ORDER BY created_at, id",
    )
    .fetch_all(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(courses)
}

pub async fn publish_course(pool: &PgPool, actor: Uuid, course_id: Uuid) -> Result<()> {
    let mut transaction = begin_as(pool, actor).await?;
    let updated = sqlx::query(
        "UPDATE courses
         SET visibility = 'public', published = TRUE,
             rights_attested_at = clock_timestamp(), published_by = $2
         WHERE id = $1 AND can_manage_course(id)",
    )
    .bind(course_id)
    .bind(actor)
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        if is_permission_denied(&error) {
            StorageError::Forbidden
        } else {
            StorageError::Database(error)
        }
    })?
    .rows_affected();
    transaction.commit().await?;
    if updated != 1 {
        return Err(StorageError::NotFound);
    }
    Ok(())
}

pub async fn enroll(pool: &PgPool, actor: Uuid, course_id: Uuid) -> Result<()> {
    let mut transaction = begin_as(pool, actor).await?;
    let result = sqlx::query(
        "INSERT INTO course_members (course_id, tenant_id, user_id, role)
         SELECT id, tenant_id, $2, 'student' FROM courses
         WHERE id = $1 AND visibility = 'public' AND published
         ON CONFLICT (course_id, user_id)
         DO UPDATE SET active = TRUE, role = 'student'",
    )
    .bind(course_id)
    .bind(actor)
    .execute(&mut *transaction)
    .await;
    let affected = match result {
        Ok(done) => done.rows_affected(),
        Err(error) if is_permission_denied(&error) => return Err(StorageError::Forbidden),
        Err(error) => return Err(StorageError::Database(error)),
    };
    transaction.commit().await?;
    if affected == 0 {
        return Err(StorageError::NotFound);
    }
    Ok(())
}

pub async fn add_course_member(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    user_id: Uuid,
    role: &str,
) -> Result<()> {
    if !matches!(role, "instructor" | "student") {
        return Err(StorageError::Invalid("role must be instructor or student"));
    }
    let mut transaction = begin_as(pool, actor).await?;
    let affected = sqlx::query(
        "INSERT INTO course_members (course_id, tenant_id, user_id, role)
         SELECT c.id, c.tenant_id, $2, $3 FROM courses c
         WHERE c.id = $1 AND can_manage_course(c.id)
         ON CONFLICT (course_id, user_id)
         DO UPDATE SET role = EXCLUDED.role, active = TRUE",
    )
    .bind(course_id)
    .bind(user_id)
    .bind(role)
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        if is_permission_denied(&error) {
            StorageError::Forbidden
        } else {
            StorageError::Database(error)
        }
    })?
    .rows_affected();
    transaction.commit().await?;
    if affected == 0 {
        return Err(StorageError::NotFound);
    }
    Ok(())
}

/// Cheap authorization probe used before an upload stream is accepted. The
/// durable write still re-checks inside its transaction, so a membership change
/// between this probe and the insert fails closed.
pub async fn can_manage(pool: &PgPool, actor: Uuid, course_id: Uuid) -> Result<bool> {
    let mut transaction = begin_as(pool, actor).await?;
    let allowed = sqlx::query_scalar::<_, bool>("SELECT can_manage_course($1)")
        .bind(course_id)
        .fetch_one(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(allowed)
}

pub async fn create_node(pool: &PgPool, actor: Uuid, course_id: Uuid, title: &str) -> Result<Uuid> {
    let mut transaction = begin_as(pool, actor).await?;
    let node_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO learning_nodes (tenant_id, course_id, title)
         SELECT tenant_id, id, $2 FROM courses WHERE id = $1 AND can_manage_course(id)
         RETURNING id",
    )
    .bind(course_id)
    .bind(title)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| {
        if is_permission_denied(&error) {
            StorageError::Forbidden
        } else {
            StorageError::Database(error)
        }
    })?
    .ok_or(StorageError::NotFound)?;
    transaction.commit().await?;
    Ok(node_id)
}

pub struct NewChallenge<'a> {
    pub prompt: &'a str,
    pub options: &'a Value,
    pub correct_index: u8,
    pub explanation: &'a str,
}

pub async fn create_challenge(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    node_id: Uuid,
    challenge: &NewChallenge<'_>,
) -> Result<Uuid> {
    if !(2..=6).contains(&challenge.options.as_array().map_or(0, Vec::len)) {
        return Err(StorageError::Invalid("challenge requires 2 to 6 options"));
    }
    let mut transaction = begin_as(pool, actor).await?;
    let challenge_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO challenges (tenant_id, course_id, node_id, prompt, options)
         SELECT c.tenant_id, c.id, n.id, $3, $4
         FROM courses c JOIN learning_nodes n ON n.course_id = c.id AND n.id = $2
         WHERE c.id = $1 AND can_manage_course(c.id)
         RETURNING id",
    )
    .bind(course_id)
    .bind(node_id)
    .bind(challenge.prompt)
    .bind(challenge.options)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| {
        if is_permission_denied(&error) {
            StorageError::Forbidden
        } else {
            StorageError::Database(error)
        }
    })?
    .ok_or(StorageError::NotFound)?;
    sqlx::query("SELECT add_challenge_answer($1, $2, $3)")
        .bind(challenge_id)
        .bind(i16::from(challenge.correct_index))
        .bind(challenge.explanation)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(challenge_id)
}

pub async fn list_content(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
) -> Result<(Vec<NodeSummary>, Vec<ChallengeSummary>)> {
    let mut transaction = begin_as(pool, actor).await?;
    let visible = sqlx::query_scalar::<_, bool>("SELECT can_read_course($1)")
        .bind(course_id)
        .fetch_one(&mut *transaction)
        .await?;
    if !visible {
        return Err(StorageError::NotFound);
    }
    let nodes = sqlx::query_as::<_, NodeSummary>(
        "SELECT id, title FROM learning_nodes WHERE course_id = $1 ORDER BY title, id",
    )
    .bind(course_id)
    .fetch_all(&mut *transaction)
    .await?;
    let challenges = sqlx::query_as::<_, ChallengeSummary>(
        "SELECT id, node_id, prompt, options FROM challenges
         WHERE course_id = $1 ORDER BY id",
    )
    .bind(course_id)
    .fetch_all(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok((nodes, challenges))
}

pub async fn practice_question(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    challenge_id: Uuid,
) -> Result<PracticeQuestion> {
    let mut transaction = begin_as(pool, actor).await?;
    let question = sqlx::query_as::<_, PracticeQuestion>(
        "SELECT c.id, c.prompt, c.options FROM challenges c
         WHERE c.id = $2 AND c.course_id = $1 AND can_study_course($1)",
    )
    .bind(course_id)
    .bind(challenge_id)
    .fetch_optional(&mut *transaction)
    .await?;
    transaction.commit().await?;
    question.ok_or(StorageError::NotFound)
}

pub async fn create_attempt(
    pool: &PgPool,
    actor: Uuid,
    course_id: Uuid,
    challenge_id: Uuid,
) -> Result<CreatedAttempt> {
    let mut transaction = begin_as(pool, actor).await?;
    let row = sqlx::query_as::<_, (Uuid, chrono::DateTime<chrono::Utc>)>(
        "INSERT INTO attempts (tenant_id, course_id, challenge_id, user_id, option_count)
         SELECT ch.tenant_id, ch.course_id, ch.id, $3, jsonb_array_length(ch.options)
         FROM challenges ch
         WHERE ch.id = $2 AND ch.course_id = $1 AND can_study_course(ch.course_id)
         RETURNING id, expires_at",
    )
    .bind(course_id)
    .bind(challenge_id)
    .bind(actor)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|error| {
        if is_permission_denied(&error) {
            StorageError::Forbidden
        } else {
            StorageError::Database(error)
        }
    })?;
    transaction.commit().await?;
    let (id, expires_at) = row.ok_or(StorageError::NotFound)?;
    Ok(CreatedAttempt { id, expires_at })
}
