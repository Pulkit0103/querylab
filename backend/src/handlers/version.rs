use axum::Json;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct VersionResponse {
    pub name: &'static str,
    pub version: &'static str,
    pub api_version: &'static str,
    /// Commit the binary was built from, when provided at build time.
    pub commit: Option<&'static str>,
}

pub async fn version() -> Json<VersionResponse> {
    Json(VersionResponse {
        name: "querylab-api",
        version: env!("CARGO_PKG_VERSION"),
        api_version: "v1",
        commit: option_env!("GIT_SHA"),
    })
}
