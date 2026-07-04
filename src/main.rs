mod api;
mod auth;
mod config;
mod gitea_origin;
mod naming;
mod oauth;
mod public_url;
mod store;
mod workspace;

use std::net::SocketAddr;

use api::{AppState, router};
use auth::AuthStore;
use axum::http::{Method, header::CONTENT_TYPE};
use config::Config;
use store::WorkspaceStore;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let port = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(3000);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    let config = Config::from_env()?;

    let state = AppState {
        config: config.clone(),
        auth: AuthStore::new(config.data_dir.clone())?,
        store: WorkspaceStore::new(&config.data_dir)?,
    };

    let app = router(state).layer(TraceLayer::new_for_http()).layer(
        CorsLayer::new()
            .allow_origin(AllowOrigin::mirror_request())
            .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
            .allow_headers([CONTENT_TYPE])
            .allow_credentials(true),
    );

    tracing::info!(%addr, data_dir = %config.data_dir.display(), "workspace-manager listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
