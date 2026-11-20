//! ER diagrams: schema graph and saved layouts.

use axum::{extract::State, http::StatusCode, Json};
use uuid::Uuid;

use crate::{
    errors::AppResult,
    models::{
        diagram::{DiagramLayout, GraphQuery, LayoutResponse, SchemaGraph},
        schema::validate_identifier,
    },
    services::{
        auth::AuthUser,
        diagrams::{DiagramService, GraphRequest},
        schema::SchemaService,
    },
    state::AppState,
    utils::validation::{ApiPath, ApiQuery, Validate, ValidatedJson, Validator},
};

fn check_schema(schema: &str) -> AppResult<()> {
    let mut v = Validator::new();
    validate_identifier(&mut v, "schema", schema);
    v.finish()
}

pub async fn graph(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<GraphQuery>,
) -> AppResult<Json<SchemaGraph>> {
    query.validate()?;
    let request = GraphRequest {
        schema: &query.schema,
        tables: query.table_list(),
        include_views: query.include_views,
        limit: query.effective_limit(),
    };
    Ok(Json(
        SchemaService::new(&state)
            .graph(user.user_id, id, request)
            .await?,
    ))
}

pub async fn get_layout(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath((id, schema)): ApiPath<(Uuid, String)>,
) -> AppResult<Json<LayoutResponse>> {
    check_schema(&schema)?;
    Ok(Json(
        DiagramService::new(&state)
            .layout(user.user_id, id, &schema)
            .await?,
    ))
}

pub async fn put_layout(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath((id, schema)): ApiPath<(Uuid, String)>,
    ValidatedJson(layout): ValidatedJson<DiagramLayout>,
) -> AppResult<Json<LayoutResponse>> {
    check_schema(&schema)?;
    Ok(Json(
        DiagramService::new(&state)
            .save_layout(user.user_id, id, &schema, layout)
            .await?,
    ))
}

pub async fn delete_layout(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath((id, schema)): ApiPath<(Uuid, String)>,
) -> AppResult<StatusCode> {
    check_schema(&schema)?;
    DiagramService::new(&state)
        .reset_layout(user.user_id, id, &schema)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
