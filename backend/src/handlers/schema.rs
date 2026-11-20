use axum::{extract::State, Json};
use uuid::Uuid;

use crate::{
    errors::AppResult,
    models::schema::{
        validate_identifier, CatalogEntry, ColumnInfo, ConstraintInfo, FunctionDetail,
        FunctionInfo, IndexInfo, Relationship, SchemaInfo, SchemaObjects, SchemaQuery, SearchHit,
        SearchQuery, TableDdl, TableInfo, TablesQuery, TriggerInfo, TriggersQuery,
    },
    services::{auth::AuthUser, schema::SchemaService},
    state::AppState,
    utils::validation::{ApiPath, ApiQuery, Validate, Validator},
};

fn check_table(schema: &SchemaQuery, table: &str) -> AppResult<()> {
    schema.validate()?;
    let mut v = Validator::new();
    validate_identifier(&mut v, "table", table);
    v.finish()
}

pub async fn schemas(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<Vec<SchemaInfo>>> {
    Ok(Json(
        SchemaService::new(&state).schemas(user.user_id, id).await?,
    ))
}

pub async fn tables(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<TablesQuery>,
) -> AppResult<Json<Vec<TableInfo>>> {
    query.validate()?;
    Ok(Json(
        SchemaService::new(&state)
            .tables(user.user_id, id, query.schema.as_deref())
            .await?,
    ))
}

pub async fn columns(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath((id, table)): ApiPath<(Uuid, String)>,
    ApiQuery(query): ApiQuery<SchemaQuery>,
) -> AppResult<Json<Vec<ColumnInfo>>> {
    check_table(&query, &table)?;
    Ok(Json(
        SchemaService::new(&state)
            .columns(user.user_id, id, &query.schema, &table)
            .await?,
    ))
}

pub async fn indexes(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath((id, table)): ApiPath<(Uuid, String)>,
    ApiQuery(query): ApiQuery<SchemaQuery>,
) -> AppResult<Json<Vec<IndexInfo>>> {
    check_table(&query, &table)?;
    Ok(Json(
        SchemaService::new(&state)
            .indexes(user.user_id, id, &query.schema, &table)
            .await?,
    ))
}

pub async fn relationships(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath((id, table)): ApiPath<(Uuid, String)>,
    ApiQuery(query): ApiQuery<SchemaQuery>,
) -> AppResult<Json<Vec<Relationship>>> {
    check_table(&query, &table)?;
    Ok(Json(
        SchemaService::new(&state)
            .relationships(user.user_id, id, &query.schema, &table)
            .await?,
    ))
}

pub async fn catalog(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> AppResult<Json<Vec<CatalogEntry>>> {
    Ok(Json(
        SchemaService::new(&state).catalog(user.user_id, id).await?,
    ))
}

pub async fn constraints(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath((id, table)): ApiPath<(Uuid, String)>,
    ApiQuery(query): ApiQuery<SchemaQuery>,
) -> AppResult<Json<Vec<ConstraintInfo>>> {
    check_table(&query, &table)?;
    Ok(Json(
        SchemaService::new(&state)
            .constraints(user.user_id, id, &query.schema, &table)
            .await?,
    ))
}

pub async fn ddl(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath((id, table)): ApiPath<(Uuid, String)>,
    ApiQuery(query): ApiQuery<SchemaQuery>,
) -> AppResult<Json<TableDdl>> {
    check_table(&query, &table)?;
    Ok(Json(
        SchemaService::new(&state)
            .ddl(user.user_id, id, &query.schema, &table)
            .await?,
    ))
}

pub async fn functions(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<TablesQuery>,
) -> AppResult<Json<Vec<FunctionInfo>>> {
    query.validate()?;
    Ok(Json(
        SchemaService::new(&state)
            .functions(user.user_id, id, query.schema.as_deref())
            .await?,
    ))
}

pub async fn function(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath((id, oid)): ApiPath<(Uuid, i64)>,
) -> AppResult<Json<FunctionDetail>> {
    Ok(Json(
        SchemaService::new(&state)
            .function(user.user_id, id, oid)
            .await?,
    ))
}

pub async fn triggers(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<TriggersQuery>,
) -> AppResult<Json<Vec<TriggerInfo>>> {
    query.validate()?;
    Ok(Json(
        SchemaService::new(&state)
            .triggers(
                user.user_id,
                id,
                query.schema.as_deref(),
                query.table.as_deref(),
            )
            .await?,
    ))
}

pub async fn objects(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<TablesQuery>,
) -> AppResult<Json<SchemaObjects>> {
    query.validate()?;
    Ok(Json(
        SchemaService::new(&state)
            .objects(user.user_id, id, query.schema.as_deref())
            .await?,
    ))
}

pub async fn search(
    State(state): State<AppState>,
    user: AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<SearchQuery>,
) -> AppResult<Json<Vec<SearchHit>>> {
    query.validate()?;
    Ok(Json(
        SchemaService::new(&state)
            .search(user.user_id, id, query.q.trim())
            .await?,
    ))
}
