//! Route table and cross-cutting HTTP middleware.

use axum::{
    extract::DefaultBodyLimit,
    http::{header, HeaderName, HeaderValue, Method},
    routing::{get, post},
    Router,
};
use tower_http::{
    catch_panic::CatchPanicLayer,
    cors::{AllowOrigin, CorsLayer},
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};

use crate::{
    handlers,
    middleware::{
        csrf,
        observability::{self, REQUEST_ID},
    },
    state::AppState,
};

/// Maximum accepted request body (SQL text is capped at 1 MiB separately).
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;

pub fn build_router(state: AppState) -> Router {
    use handlers::{
        auth, connections, data, data_jobs, diagrams, library, migrations, monitoring, performance,
        query, schema, workspace,
    };

    let api = Router::new()
        .route("/health", get(handlers::health::health))
        .route("/version", get(handlers::version::version))
        // Authentication
        .route("/auth/register", post(auth::register))
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/me", get(auth::me))
        // Database connections (all require an authenticated user)
        .route(
            "/connections",
            get(connections::list).post(connections::create),
        )
        .route("/connections/test", post(connections::test_draft))
        .route(
            "/connections/{id}",
            get(connections::get)
                .put(connections::update)
                .delete(connections::delete),
        )
        .route("/connections/{id}/test", post(connections::test_saved))
        .route("/connections/{id}/duplicate", post(connections::duplicate))
        .route(
            "/connection-groups",
            get(connections::list_groups).post(connections::create_group),
        )
        .route(
            "/connection-groups/{id}",
            axum::routing::put(connections::update_group).delete(connections::delete_group),
        )
        // Schema explorer
        .route("/connections/{id}/schemas", get(schema::schemas))
        .route("/connections/{id}/tables", get(schema::tables))
        .route("/connections/{id}/catalog", get(schema::catalog))
        .route(
            "/connections/{id}/tables/{table}/columns",
            get(schema::columns),
        )
        .route(
            "/connections/{id}/tables/{table}/indexes",
            get(schema::indexes),
        )
        .route(
            "/connections/{id}/tables/{table}/relationships",
            get(schema::relationships),
        )
        .route(
            "/connections/{id}/tables/{table}/constraints",
            get(schema::constraints),
        )
        .route("/connections/{id}/tables/{table}/ddl", get(schema::ddl))
        .route("/connections/{id}/functions", get(schema::functions))
        .route("/connections/{id}/functions/{oid}", get(schema::function))
        .route("/connections/{id}/triggers", get(schema::triggers))
        .route("/connections/{id}/objects", get(schema::objects))
        .route("/connections/{id}/search", get(schema::search))
        // ER diagrams
        .route("/connections/{id}/schema-graph", get(diagrams::graph))
        .route(
            "/connections/{id}/diagram-layouts/{schema}",
            get(diagrams::get_layout)
                .put(diagrams::put_layout)
                .delete(diagrams::delete_layout),
        )
        // Monitoring
        .route(
            "/connections/{id}/monitor/activity",
            get(monitoring::activity),
        )
        .route("/connections/{id}/monitor/locks", get(monitoring::locks))
        .route("/connections/{id}/monitor/sizes", get(monitoring::sizes))
        .route(
            "/connections/{id}/monitor/top-queries",
            get(monitoring::top_queries),
        )
        .route(
            "/connections/{id}/monitor/backends/{pid}/cancel",
            post(monitoring::cancel),
        )
        .route(
            "/connections/{id}/monitor/backends/{pid}/terminate",
            post(monitoring::terminate),
        )
        // Data Studio
        .route("/connections/{id}/data/browse", post(data::browse))
        .route("/connections/{id}/data/preview", post(data::preview))
        .route("/connections/{id}/data/apply", post(data::apply))
        .route("/connections/{id}/data/export", post(data::export))
        // Import, export and mock data
        .route("/connections/{id}/exports", post(data_jobs::export))
        .route("/connections/{id}/imports", post(data_jobs::create_import))
        .route(
            "/imports/{id}/data",
            axum::routing::put(data_jobs::upload).layer(DefaultBodyLimit::max(
                crate::models::data_jobs::MAX_UPLOAD_CHUNK_BYTES + 1024,
            )),
        )
        .route("/imports/{id}/analyze", post(data_jobs::analyze))
        .route("/imports/{id}/check", post(data_jobs::check))
        .route("/imports/{id}/run", post(data_jobs::run_import))
        .route(
            "/imports/{id}",
            get(data_jobs::import_status).delete(data_jobs::delete_import),
        )
        .route(
            "/connections/{id}/mock-data/preview",
            post(data_jobs::mock_preview),
        )
        .route("/connections/{id}/mock-data/run", post(data_jobs::mock_run))
        .route("/data-jobs", get(data_jobs::list_jobs))
        .route("/data-jobs/{id}", get(data_jobs::get_job))
        .route("/data-jobs/{id}/cancel", post(data_jobs::cancel_job))
        // SQL execution
        .route("/query/execute", post(query::execute))
        .route("/query/explain", post(query::explain))
        .route("/query/{execution_id}/cancel", post(query::cancel))
        // History
        .route(
            "/history",
            get(library::list_history).delete(library::clear_history),
        )
        .route(
            "/history/{id}",
            get(library::get_history).delete(library::delete_history),
        )
        .route(
            "/history/{id}/pin",
            axum::routing::put(library::pin_history),
        )
        // Saved queries and collections
        .route(
            "/queries",
            get(library::list_saved).post(library::create_saved),
        )
        .route(
            "/queries/{id}",
            get(library::get_saved)
                .put(library::update_saved)
                .delete(library::delete_saved),
        )
        .route("/queries/{id}/pin", axum::routing::put(library::pin_saved))
        .route("/tags", get(library::tags))
        .route(
            "/collections",
            get(library::list_collections).post(library::create_collection),
        )
        .route(
            "/collections/{id}",
            axum::routing::put(library::update_collection).delete(library::delete_collection),
        )
        .route("/stats", get(library::stats))
        // Schema diff and migrations
        .route("/schema-diff", post(migrations::schema_diff))
        .route(
            "/migration-projects",
            get(migrations::list_projects).post(migrations::create_project),
        )
        .route(
            "/migration-projects/{id}",
            axum::routing::delete(migrations::delete_project),
        )
        .route(
            "/migration-projects/{id}/migrations",
            get(migrations::list_migrations).post(migrations::create_migration),
        )
        .route("/migration-projects/{id}/status", get(migrations::status))
        .route(
            "/migrations/{id}",
            get(migrations::get_migration)
                .put(migrations::update_migration)
                .delete(migrations::delete_migration),
        )
        .route("/migrations/{id}/apply", post(migrations::apply))
        .route("/migrations/{id}/rollback", post(migrations::rollback))
        .route("/migration-runs", get(migrations::runs))
        // Performance
        .route("/performance/fingerprints", get(performance::fingerprints))
        .route(
            "/performance/fingerprints/{fingerprint}",
            get(performance::timeline),
        )
        .route(
            "/plans",
            get(performance::list_plans).post(performance::create_plan),
        )
        .route(
            "/plans/{id}",
            get(performance::get_plan).delete(performance::delete_plan),
        )
        // Workspace
        .route(
            "/workspace/tabs",
            get(workspace::get_tabs).put(workspace::put_tabs),
        )
        .route(
            "/workspace/settings",
            get(workspace::get_settings).put(workspace::put_settings),
        );

    // HSTS only makes sense when the API is served over HTTPS, which is
    // exactly when Secure cookies are enabled.
    let hsts = state.config.cookie_secure;

    // Layers listed later wrap the ones before them (outermost last).
    Router::new()
        .route("/health", get(handlers::health::health))
        .nest("/api", api)
        .fallback(handlers::not_found)
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        // Runs inside CORS so preflight requests are answered first.
        .layer(axum::middleware::from_fn(csrf::require_csrf_header))
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            state.config.request_timeout,
        ))
        .layer(cors_layer(&state.config.cors_origins))
        .layer(CatchPanicLayer::custom(observability::panic_response))
        .layer(axum::middleware::map_response(
            move |mut res: axum::response::Response| async move {
                observability::apply_security_headers(res.headers_mut(), hsts);
                res
            },
        ))
        .layer(PropagateRequestIdLayer::new(REQUEST_ID))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(observability::RequestSpan)
                .on_request(())
                .on_response(observability::LogResponse)
                // 5xx responses are already logged by `LogResponse`.
                .on_failure(()),
        )
        .layer(SetRequestIdLayer::new(REQUEST_ID, MakeRequestUuid))
        .with_state(state)
}

fn cors_layer(origins: &[String]) -> CorsLayer {
    let allowed: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|o| HeaderValue::from_str(o).ok())
        .collect();
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(allowed))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            HeaderName::from_static(csrf::CSRF_HEADER),
        ])
        .allow_credentials(true)
}
