//! Saved ER diagram positions, per connection and schema (shared by
//! everyone who can see the connection).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use sqlx::{types::Json, PgPool};
use uuid::Uuid;

use crate::models::diagram::NodePosition;

pub type Positions = BTreeMap<String, NodePosition>;

pub async fn get_layout(
    db: &PgPool,
    connection_id: Uuid,
    schema: &str,
) -> Result<Option<(Positions, DateTime<Utc>)>, sqlx::Error> {
    let row: Option<(Json<Positions>, DateTime<Utc>)> = sqlx::query_as(
        "SELECT positions, updated_at FROM diagram_layouts \
         WHERE connection_id = $1 AND schema_name = $2",
    )
    .bind(connection_id)
    .bind(schema)
    .fetch_optional(db)
    .await?;
    Ok(row.map(|(Json(p), at)| (p, at)))
}

pub async fn put_layout(
    db: &PgPool,
    user_id: Uuid,
    connection_id: Uuid,
    schema: &str,
    positions: &Positions,
) -> Result<DateTime<Utc>, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO diagram_layouts (user_id, connection_id, schema_name, positions, updated_at) \
         VALUES ($1, $2, $3, $4, now()) \
         ON CONFLICT (connection_id, schema_name) \
         DO UPDATE SET positions = EXCLUDED.positions, user_id = EXCLUDED.user_id, updated_at = now() \
         RETURNING updated_at",
    )
    .bind(user_id)
    .bind(connection_id)
    .bind(schema)
    .bind(Json(positions))
    .fetch_one(db)
    .await
}

pub async fn delete_layout(
    db: &PgPool,
    connection_id: Uuid,
    schema: &str,
) -> Result<bool, sqlx::Error> {
    let done =
        sqlx::query("DELETE FROM diagram_layouts WHERE connection_id = $1 AND schema_name = $2")
            .bind(connection_id)
            .bind(schema)
            .execute(db)
            .await?;
    Ok(done.rows_affected() > 0)
}
