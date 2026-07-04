use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION, header::SET_COOKIE};
use axum::response::{Html, IntoResponse, Redirect};
use axum::routing::{get, post};
use axum::{Json, Router};
use rand::Rng;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path as FsPath, PathBuf};
use std::time::Duration;
use tokio::process::Command;
use url::Url;

use crate::auth::{AuthStore, PendingOAuth, UserSession};
use crate::config::{Config, WorkspaceSharedData};
use crate::gitea_origin::infer_gitea_base_url;
use crate::naming::slug_component;
use crate::oauth::{
    AuthStartQuery, OAuthCallbackQuery, build_authorize_url, exchange_code_for_token,
    fetch_gitea_user, session_cookie_header, session_id_from_headers,
};
use crate::public_url::infer_manager_base_url;
use crate::store::WorkspaceStore;
use crate::workspace::{CreateWorkspaceRequest, Workspace, WorkspaceQuery, WorkspaceStatus};

#[derive(Debug, Clone)]
pub struct AppState {
    pub config: Config,
    pub auth: AuthStore,
    pub store: WorkspaceStore,
}

#[derive(Debug, Clone, Copy)]
enum AuthCheckMode {
    Cached,
    Fresh,
}

const TOKEN_CHECK_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Serialize)]
struct HealthResponse {
    ok: bool,
    service: &'static str,
    data_dir: String,
}

#[derive(Debug, Serialize)]
struct WorkspaceListResponse {
    exists: bool,
    count: usize,
    workspaces: Vec<Workspace>,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Serialize)]
struct AuthRequiredResponse {
    error: String,
    login_url: String,
}

#[derive(Debug, Serialize)]
struct MeResponse {
    authenticated: bool,
    gitea_base_url: String,
    user: crate::auth::GiteaUser,
}

#[derive(Debug, Deserialize)]
struct GitCredentialRequest {
    protocol: String,
    host: String,
    path: String,
}

#[derive(Debug, Serialize)]
struct GitCredentialResponse {
    username: String,
    password: String,
}

const AUTH_SUCCESS_HTML: &str = r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <title>授权成功</title>
  <style>
    body { font-family: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; margin: 48px; color: #24292f; }
    button { padding: 8px 14px; border: 1px solid #d0d7de; border-radius: 6px; background: #f6f8fa; cursor: pointer; }
  </style>
</head>
<body>
  <h2>授权成功</h2>
  <p>Code Spaces 已完成授权，可以关闭此窗口。</p>
  <button onclick="window.close()">关闭窗口</button>
  <script>setTimeout(() => window.close(), 1200);</script>
</body>
</html>"#;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/auth/gitea/start", get(auth_start))
        .route("/auth/gitea/callback", get(auth_callback))
        .route("/api/v1/me", get(me))
        .route("/api/v1/git/credentials", post(git_credentials))
        .route(
            "/api/v1/workspaces",
            get(list_workspaces).post(create_workspace),
        )
        .route(
            "/api/v1/workspaces/:workspace_id",
            get(get_workspace).delete(delete_workspace),
        )
        .route(
            "/api/v1/workspaces/:workspace_id/start",
            post(start_workspace),
        )
        .route(
            "/api/v1/workspaces/:workspace_id/stop",
            post(stop_workspace),
        )
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: true,
        service: "workspace-manager",
        data_dir: state.config.data_dir.display().to_string(),
    })
}

async fn create_workspace(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateWorkspaceRequest>,
) -> impl IntoResponse {
    let session = match require_session(&state, &headers, AuthCheckMode::Fresh).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    if req.repo.trim().is_empty() {
        return error(StatusCode::BAD_REQUEST, "repo is required");
    }

    let Some(session_id) = session_id_from_headers(&headers) else {
        return auth_required(&state, &headers);
    };

    let workspace = state
        .store
        .create(
            req,
            session.user.id.to_string(),
            session_id,
            session.gitea_base_url.clone(),
        )
        .await;
    let mut workspace = workspace;
    if state.config.workspace.code_server_auth.as_deref() == Some("none") {
        workspace.code_server_password = None;
    } else if let Some(password) = state.config.workspace.code_server_password.as_ref() {
        workspace.code_server_password = Some(password.clone());
    }
    let workspace = state
        .store
        .set_code_server_password(
            &workspace.workspace_id,
            workspace.code_server_password.clone(),
        )
        .await
        .unwrap_or(workspace);
    let workspace_url =
        match start_workspace_container(&state, &headers, &workspace, &session).await {
            Ok(url) => url,
            Err(message) => {
                let _ = state
                    .store
                    .mark(&workspace.workspace_id, WorkspaceStatus::Error, None)
                    .await;
                return error(StatusCode::INTERNAL_SERVER_ERROR, &message);
            }
        };

    let workspace = state
        .store
        .mark(
            &workspace.workspace_id,
            WorkspaceStatus::Running,
            Some(workspace_url),
        )
        .await
        .unwrap_or(workspace);

    (StatusCode::CREATED, Json(workspace)).into_response()
}

async fn list_workspaces(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<WorkspaceQuery>,
) -> impl IntoResponse {
    let session = match require_session(&state, &headers, AuthCheckMode::Cached).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    let query = WorkspaceQuery {
        user_id: Some(session.user.id.to_string()),
        ..query
    };
    let mut workspaces = state.store.list(&query).await;
    workspaces.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    Json(WorkspaceListResponse {
        exists: !workspaces.is_empty(),
        count: workspaces.len(),
        workspaces,
    })
    .into_response()
}

async fn get_workspace(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(workspace_id): Path<String>,
) -> impl IntoResponse {
    let session = match require_session(&state, &headers, AuthCheckMode::Cached).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    match state.store.get(&workspace_id).await {
        Some(workspace) if workspace.user_id == session.user.id.to_string() => {
            Json(workspace).into_response()
        }
        Some(_) => error(StatusCode::FORBIDDEN, "workspace belongs to another user"),
        None => error(StatusCode::NOT_FOUND, "workspace not found"),
    }
}

async fn start_workspace(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(workspace_id): Path<String>,
) -> impl IntoResponse {
    mark_workspace(state, headers, workspace_id, WorkspaceStatus::Running).await
}

async fn stop_workspace(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(workspace_id): Path<String>,
) -> impl IntoResponse {
    mark_workspace(state, headers, workspace_id, WorkspaceStatus::Stopped).await
}

async fn delete_workspace(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(workspace_id): Path<String>,
) -> impl IntoResponse {
    let session = match require_session(&state, &headers, AuthCheckMode::Fresh).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    match state.store.get(&workspace_id).await {
        Some(workspace) if workspace.user_id == session.user.id.to_string() => {}
        Some(_) => return error(StatusCode::FORBIDDEN, "workspace belongs to another user"),
        None => return error(StatusCode::NOT_FOUND, "workspace not found"),
    }

    match state.store.remove(&workspace_id).await {
        Some(workspace) => {
            remove_workspace_container(&workspace).await;
            Json(workspace).into_response()
        }
        None => error(StatusCode::NOT_FOUND, "workspace not found"),
    }
}

async fn mark_workspace(
    state: AppState,
    headers: HeaderMap,
    workspace_id: String,
    status: WorkspaceStatus,
) -> axum::response::Response {
    let session = match require_session(&state, &headers, AuthCheckMode::Fresh).await {
        Ok(session) => session,
        Err(response) => return response,
    };

    match state.store.get(&workspace_id).await {
        Some(workspace) if workspace.user_id == session.user.id.to_string() => {}
        Some(_) => return error(StatusCode::FORBIDDEN, "workspace belongs to another user"),
        None => return error(StatusCode::NOT_FOUND, "workspace not found"),
    }

    let url = if status == WorkspaceStatus::Running {
        Some(format!("/spaces/{workspace_id}/"))
    } else {
        None
    };

    match state.store.mark(&workspace_id, status, url).await {
        Some(workspace) => Json(workspace).into_response(),
        None => error(StatusCode::NOT_FOUND, "workspace not found"),
    }
}

async fn remove_workspace_container(workspace: &Workspace) {
    let _ = Command::new("docker")
        .args(["rm", "-f", &workspace.container_name])
        .output()
        .await;
}

async fn start_workspace_container(
    state: &AppState,
    headers: &HeaderMap,
    workspace: &Workspace,
    session: &UserSession,
) -> Result<String, String> {
    let manager_url = infer_manager_base_url(
        headers,
        state
            .config
            .auth
            .as_ref()
            .and_then(|auth| auth.public_url.as_deref()),
    )
    .ok_or_else(|| "cannot infer manager public url".to_string())?;

    let clone_url = workspace
        .clone_url
        .as_deref()
        .ok_or_else(|| "clone_url is required".to_string())?;

    let git_user_name = session
        .user
        .full_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&session.user.login);
    let git_user_email = session
        .user
        .email
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("{}.noreply.local", session.user.login));

    let mut last_error = None;
    for _ in 0..20 {
        let port = random_workspace_port(
            state.config.workspace.port_start,
            state.config.workspace.port_end,
        );
        let mut args = vec![
            "run".to_string(),
            "-d".to_string(),
            "--name".to_string(),
            workspace.container_name.clone(),
            "-p".to_string(),
            format!("0.0.0.0:{port}:8080"),
            "-e".to_string(),
            format!("WORKSPACE_TOKEN={}", workspace.workspace_token),
            "-e".to_string(),
            format!("WORKSPACE_MANAGER_URL={manager_url}"),
            "-e".to_string(),
            format!("CLONE_URL={clone_url}"),
            "-e".to_string(),
            format!("GIT_USER_NAME={git_user_name}"),
            "-e".to_string(),
            format!("GIT_USER_EMAIL={git_user_email}"),
        ];

        if state.config.workspace.code_server_auth.as_deref() == Some("none") {
            args.push("-e".to_string());
            args.push("CODE_SERVER_AUTH=none".to_string());
        } else if let Some(password) = workspace.code_server_password.as_deref() {
            args.push("-e".to_string());
            args.push(format!("PASSWORD={password}"));
        }

        add_shared_code_server_mounts(
            &mut args,
            &state.config.data_dir,
            state.config.workspace.shared_data,
            &state.config.workspace.shared_files,
            &state.config.workspace.shared_excludes,
            &workspace.workspace_id,
            &session.user.login,
        )?;

        args.push(state.config.workspace.image.clone());

        let output = Command::new("docker")
            .args(args)
            .output()
            .await
            .map_err(|error| format!("failed to run docker: {error}"))?;

        if output.status.success() {
            return workspace_public_url(&manager_url, port);
        }

        last_error = Some(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    Err(last_error.unwrap_or_else(|| "failed to allocate workspace port".to_string()))
}

fn shared_code_server_data_dir(
    data_dir: &FsPath,
    strategy: WorkspaceSharedData,
    user_login: &str,
) -> Option<PathBuf> {
    match strategy {
        WorkspaceSharedData::None => None,
        WorkspaceSharedData::Global => Some(data_dir.join("shared-code-server/global")),
        WorkspaceSharedData::User => Some(
            data_dir
                .join("shared-code-server/users")
                .join(slug_component(user_login, 48)),
        ),
    }
}

fn add_shared_code_server_mounts(
    args: &mut Vec<String>,
    data_dir: &FsPath,
    strategy: WorkspaceSharedData,
    shared_files: &[PathBuf],
    shared_excludes: &[PathBuf],
    workspace_id: &str,
    user_login: &str,
) -> Result<(), String> {
    let Some(shared_dir) = shared_code_server_data_dir(data_dir, strategy, user_login) else {
        return Ok(());
    };

    if shared_files.is_empty() {
        fs::create_dir_all(&shared_dir)
            .map_err(|error| format!("failed to create shared data dir: {error}"))?;
        ensure_bind_mount_permissions(&shared_dir)?;
        let shared_dir = fs::canonicalize(&shared_dir)
            .map_err(|error| format!("failed to resolve shared data dir: {error}"))?;
        args.push("-v".to_string());
        args.push(format!(
            "{}:/home/coder/.local/share/code-server",
            shared_dir.display()
        ));
    } else {
        for container_path in shared_files {
            add_shared_code_server_path_mount(args, &shared_dir, container_path)?;
        }
    }

    add_shared_code_server_excludes(args, data_dir, workspace_id, shared_excludes)?;
    Ok(())
}

fn add_shared_code_server_path_mount(
    args: &mut Vec<String>,
    shared_dir: &FsPath,
    container_path: &FsPath,
) -> Result<(), String> {
    validate_shared_file_path(container_path)?;
    let storage_path = shared_file_storage_path(container_path)?;
    let mount_path = shared_file_container_path(container_path);
    let host_path = shared_dir.join(&storage_path);
    let is_file_path = shared_path_is_probably_file(container_path);

    if !host_path.exists() {
        if is_file_path {
            if let Some(parent) = host_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("failed to create shared file parent: {error}"))?;
                ensure_bind_mount_permissions(parent)?;
            }
            fs::write(&host_path, b"{}\n")
                .map_err(|error| format!("failed to create shared file: {error}"))?;
        } else {
            fs::create_dir_all(&host_path)
                .map_err(|error| format!("failed to create shared dir: {error}"))?;
        }
    }

    ensure_bind_mount_permissions(&host_path)?;
    let host_path = fs::canonicalize(&host_path)
        .map_err(|error| format!("failed to resolve shared path: {error}"))?;
    args.push("-v".to_string());
    args.push(format!("{}:{}", host_path.display(), mount_path.display()));

    Ok(())
}

fn shared_file_storage_path(path: &FsPath) -> Result<PathBuf, String> {
    if path.is_absolute() {
        let mut storage_path = PathBuf::from("__absolute");
        for component in path.components() {
            match component {
                std::path::Component::RootDir => {}
                std::path::Component::Normal(value) => storage_path.push(value),
                _ => return Err("shared file path contains unsupported component".to_string()),
            }
        }
        if storage_path == PathBuf::from("__absolute") {
            return Err("shared file path must not be root".to_string());
        }
        Ok(storage_path)
    } else {
        Ok(path.to_path_buf())
    }
}

fn shared_file_container_path(path: &FsPath) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        PathBuf::from("/home/coder/.local/share/code-server").join(path)
    }
}

fn shared_path_is_probably_file(path: &FsPath) -> bool {
    path.extension().is_some()
}

fn add_shared_code_server_excludes(
    args: &mut Vec<String>,
    data_dir: &FsPath,
    workspace_id: &str,
    shared_excludes: &[PathBuf],
) -> Result<(), String> {
    for relative_path in shared_excludes {
        validate_shared_file_path(relative_path)?;
        let host_path = data_dir
            .join("workspaces")
            .join(workspace_id)
            .join("private-code-server")
            .join(relative_path);
        fs::create_dir_all(&host_path)
            .map_err(|error| format!("failed to create private shared exclude dir: {error}"))?;
        ensure_bind_mount_permissions(&host_path)?;
        let host_path = fs::canonicalize(&host_path)
            .map_err(|error| format!("failed to resolve private shared exclude dir: {error}"))?;
        args.push("-v".to_string());
        args.push(format!(
            "{}:/home/coder/.local/share/code-server/{}",
            host_path.display(),
            relative_path.display()
        ));
    }
    Ok(())
}

fn ensure_bind_mount_permissions(path: &FsPath) -> Result<(), String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("failed to inspect bind mount path permissions: {error}"))?;
    let mode = if metadata.is_dir() { 0o777 } else { 0o666 };
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .map_err(|error| format!("failed to update bind mount path permissions: {error}"))?;

    if metadata.is_dir() {
        for entry in fs::read_dir(path)
            .map_err(|error| format!("failed to read bind mount dir permissions: {error}"))?
        {
            let entry = entry.map_err(|error| {
                format!("failed to read bind mount dir entry permissions: {error}")
            })?;
            let entry_path = entry.path();
            let entry_type = entry.file_type().map_err(|error| {
                format!("failed to inspect bind mount dir entry permissions: {error}")
            })?;
            if entry_type.is_symlink() {
                continue;
            }
            ensure_bind_mount_permissions(&entry_path)?;
        }
    }

    Ok(())
}

fn validate_shared_file_path(path: &FsPath) -> Result<(), String> {
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("shared file path must not contain ..".to_string());
    }
    Ok(())
}

fn random_workspace_port(start: u16, end: u16) -> u16 {
    if start == end {
        return start;
    }
    rand::thread_rng().gen_range(start..=end)
}

fn workspace_public_url(manager_url: &str, port: u16) -> Result<String, String> {
    let mut url = Url::parse(manager_url).map_err(|_| "invalid manager public url".to_string())?;
    url.set_port(Some(port))
        .map_err(|_| "failed to set workspace port".to_string())?;
    url.set_path("/");
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.to_string())
}

async fn git_credentials(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<GitCredentialRequest>,
) -> impl IntoResponse {
    let Some(workspace_token) = bearer_token(&headers) else {
        return error(StatusCode::UNAUTHORIZED, "workspace token is required");
    };

    let Some(workspace) = state.store.get_by_workspace_token(workspace_token).await else {
        return error(StatusCode::UNAUTHORIZED, "invalid workspace token");
    };

    if !git_request_matches_workspace(&req, &workspace) {
        return error(
            StatusCode::FORBIDDEN,
            "git credential request is outside this workspace",
        );
    }

    let Some(session) = state.auth.get_session(&workspace.session_id).await else {
        return error(
            StatusCode::UNAUTHORIZED,
            "workspace owner is not authenticated",
        );
    };

    if session.user.id.to_string() != workspace.user_id
        || session.gitea_base_url != workspace.gitea_base_url
    {
        return error(
            StatusCode::UNAUTHORIZED,
            "workspace session does not match owner",
        );
    }

    let client = Client::new();
    match fetch_gitea_user(&client, &session.gitea_base_url, &session.access_token).await {
        Ok(user) if user.id == session.user.id => Json(GitCredentialResponse {
            username: session.user.login,
            password: session.access_token,
        })
        .into_response(),
        _ => {
            let _ = state.auth.remove_session(&workspace.session_id).await;
            error(StatusCode::UNAUTHORIZED, "workspace owner token is invalid")
        }
    }
}

async fn auth_start(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AuthStartQuery>,
) -> impl IntoResponse {
    let Some(auth) = state.config.auth.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "oauth is not configured");
    };

    let Some(gitea_base_url) = query
        .gitea_base_url
        .as_deref()
        .and_then(normalize_http_base_url)
        .or_else(|| infer_gitea_base_url(&headers))
    else {
        return error(StatusCode::BAD_REQUEST, "cannot infer gitea base url");
    };

    let Some(manager_base_url) = infer_manager_base_url(&headers, auth.public_url.as_deref())
    else {
        return error(StatusCode::BAD_REQUEST, "cannot infer manager public url");
    };

    let redirect_uri = format!("{manager_base_url}/auth/gitea/callback");
    let return_to = query
        .return_to
        .as_deref()
        .and_then(|value| safe_return_to(&gitea_base_url, value));
    let state_id = state
        .auth
        .create_oauth_state(PendingOAuth {
            gitea_base_url: gitea_base_url.clone(),
            redirect_uri: redirect_uri.clone(),
            return_to,
            popup: query.popup.unwrap_or(false),
        })
        .await;

    match build_authorize_url(&gitea_base_url, auth, &redirect_uri, &state_id) {
        Ok(url) => Redirect::temporary(&url).into_response(),
        Err(_) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed to build authorize url",
        ),
    }
}

async fn auth_callback(
    State(state): State<AppState>,
    Query(query): Query<OAuthCallbackQuery>,
) -> impl IntoResponse {
    let Some(auth) = state.config.auth.as_ref() else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "oauth is not configured");
    };

    let Some(pending) = state.auth.take_oauth_state(&query.state).await else {
        return error(StatusCode::BAD_REQUEST, "invalid oauth state");
    };

    let client = Client::new();
    let access_token = match exchange_code_for_token(
        &client,
        &pending.gitea_base_url,
        auth,
        &query.code,
        &pending.redirect_uri,
    )
    .await
    {
        Ok(access_token) => access_token,
        Err(_) => return error(StatusCode::BAD_GATEWAY, "failed to exchange oauth code"),
    };

    let user = match fetch_gitea_user(&client, &pending.gitea_base_url, &access_token).await {
        Ok(user) => user,
        Err(_) => return error(StatusCode::BAD_GATEWAY, "failed to fetch gitea user"),
    };

    let session_id = match state
        .auth
        .create_session(UserSession {
            gitea_base_url: pending.gitea_base_url.clone(),
            access_token,
            user,
        })
        .await
    {
        Ok(session_id) => session_id,
        Err(_) => {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to create session",
            );
        }
    };

    if pending.popup {
        let mut response = Html(AUTH_SUCCESS_HTML).into_response();
        response
            .headers_mut()
            .insert(SET_COOKIE, session_cookie_header(&session_id));
        return response;
    }

    let location = pending.return_to.unwrap_or(pending.gitea_base_url);
    let mut response = Redirect::temporary(&location).into_response();
    response
        .headers_mut()
        .insert(SET_COOKIE, session_cookie_header(&session_id));
    response
}

async fn me(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    match require_session(&state, &headers, AuthCheckMode::Fresh).await {
        Ok(session) => Json(MeResponse {
            authenticated: true,
            gitea_base_url: session.gitea_base_url,
            user: session.user,
        })
        .into_response(),
        Err(response) => response,
    }
}

async fn require_session(
    state: &AppState,
    headers: &HeaderMap,
    mode: AuthCheckMode,
) -> Result<UserSession, axum::response::Response> {
    let Some(session_id) = session_id_from_headers(headers) else {
        return Err(auth_required(state, headers));
    };

    let Some(session) = state.auth.get_session(&session_id).await else {
        return Err(auth_required(state, headers));
    };

    let needs_check = match mode {
        AuthCheckMode::Fresh => true,
        AuthCheckMode::Cached => {
            !state
                .auth
                .token_check_is_fresh(&session_id, TOKEN_CHECK_TTL)
                .await
        }
    };

    if !needs_check {
        return Ok(session);
    }

    let client = Client::new();
    match fetch_gitea_user(&client, &session.gitea_base_url, &session.access_token).await {
        Ok(user) if user.id == session.user.id => {
            state.auth.mark_token_checked(&session_id).await;
            Ok(UserSession { user, ..session })
        }
        _ => {
            let _ = state.auth.remove_session(&session_id).await;
            Err(auth_required(state, headers))
        }
    }
}

fn auth_required(state: &AppState, headers: &HeaderMap) -> axum::response::Response {
    let login_url =
        build_login_url(state, headers).unwrap_or_else(|| "/auth/gitea/start".to_string());
    (
        StatusCode::UNAUTHORIZED,
        Json(AuthRequiredResponse {
            error: "authentication required".to_string(),
            login_url,
        }),
    )
        .into_response()
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|token| !token.is_empty())
}

fn git_request_matches_workspace(req: &GitCredentialRequest, workspace: &Workspace) -> bool {
    if req.protocol != "http" && req.protocol != "https" {
        return false;
    }

    let requested_path = req.path.trim_start_matches('/').trim_end_matches(".git");
    if requested_path != workspace.repo.trim_end_matches(".git") {
        return false;
    }

    let Some(clone_url) = workspace.clone_url.as_deref() else {
        return true;
    };

    let Ok(clone_url) = Url::parse(clone_url) else {
        return false;
    };

    let clone_host = match clone_url.port() {
        Some(port) => format!("{}:{port}", clone_url.host_str().unwrap_or_default()),
        None => clone_url.host_str().unwrap_or_default().to_string(),
    };

    clone_url.scheme() == req.protocol
        && clone_host == req.host
        && clone_url
            .path()
            .trim_start_matches('/')
            .trim_end_matches(".git")
            == requested_path
}

fn build_login_url(state: &AppState, headers: &HeaderMap) -> Option<String> {
    let auth = state.config.auth.as_ref();
    let manager_base_url =
        infer_manager_base_url(headers, auth.and_then(|auth| auth.public_url.as_deref()))?;
    let return_to = headers
        .get("referer")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("/");
    let gitea_base_url = infer_gitea_base_url(headers)?;

    let mut url = Url::parse(&format!("{manager_base_url}/auth/gitea/start")).ok()?;
    url.query_pairs_mut().append_pair("return_to", return_to);
    url.query_pairs_mut()
        .append_pair("gitea_base_url", &gitea_base_url);
    Some(url.to_string())
}

fn normalize_http_base_url(value: &str) -> Option<String> {
    let value = value.trim().trim_end_matches('/');
    if value.starts_with("http://") || value.starts_with("https://") {
        Some(value.to_string())
    } else {
        None
    }
}

fn safe_return_to(gitea_base_url: &str, value: &str) -> Option<String> {
    if value.starts_with('/') && !value.starts_with("//") {
        return Some(value.to_string());
    }

    let gitea = Url::parse(gitea_base_url).ok()?;
    let target = Url::parse(value).ok()?;
    if target.scheme() == gitea.scheme()
        && target.host_str() == gitea.host_str()
        && target.port_or_known_default() == gitea.port_or_known_default()
    {
        Some(target.to_string())
    } else {
        None
    }
}

fn error(status: StatusCode, message: &str) -> axum::response::Response {
    (
        status,
        Json(ErrorResponse {
            error: message.to_string(),
        }),
    )
        .into_response()
}
