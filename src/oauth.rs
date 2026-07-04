use axum::http::{HeaderMap, HeaderValue};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::auth::GiteaUser;
use crate::config::AuthConfig;

pub const SESSION_COOKIE: &str = "workspace_manager_session";

#[derive(Debug, Deserialize)]
pub struct OAuthCallbackQuery {
    pub code: String,
    pub state: String,
}

#[derive(Debug, Deserialize)]
pub struct AuthStartQuery {
    pub return_to: Option<String>,
    pub gitea_base_url: Option<String>,
    pub popup: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Debug, Serialize)]
struct TokenRequest<'a> {
    client_id: &'a str,
    client_secret: &'a str,
    code: &'a str,
    grant_type: &'static str,
    redirect_uri: &'a str,
}

pub fn build_authorize_url(
    gitea_base_url: &str,
    auth: &AuthConfig,
    redirect_uri: &str,
    state: &str,
) -> anyhow::Result<String> {
    let mut url = Url::parse(&format!(
        "{}/login/oauth/authorize",
        gitea_base_url.trim_end_matches('/')
    ))?;

    url.query_pairs_mut()
        .append_pair("client_id", &auth.client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("state", state);

    Ok(url.to_string())
}

pub async fn exchange_code_for_token(
    client: &Client,
    gitea_base_url: &str,
    auth: &AuthConfig,
    code: &str,
    redirect_uri: &str,
) -> anyhow::Result<String> {
    let url = format!(
        "{}/login/oauth/access_token",
        gitea_base_url.trim_end_matches('/')
    );

    let response = client
        .post(url)
        .header("Accept", "application/json")
        .json(&TokenRequest {
            client_id: &auth.client_id,
            client_secret: &auth.client_secret,
            code,
            grant_type: "authorization_code",
            redirect_uri,
        })
        .send()
        .await?
        .error_for_status()?;

    Ok(response.json::<TokenResponse>().await?.access_token)
}

pub async fn fetch_gitea_user(
    client: &Client,
    gitea_base_url: &str,
    access_token: &str,
) -> anyhow::Result<GiteaUser> {
    let url = format!("{}/api/v1/user", gitea_base_url.trim_end_matches('/'));
    let user = client
        .get(url)
        .bearer_auth(access_token)
        .send()
        .await?
        .error_for_status()?
        .json::<GiteaUser>()
        .await?;

    Ok(user)
}

pub fn session_cookie_header(session_id: &str) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{SESSION_COOKIE}={session_id}; Path=/; HttpOnly; SameSite=Lax"
    ))
    .expect("session cookie is ascii")
}

pub fn session_id_from_headers(headers: &HeaderMap) -> Option<String> {
    let cookie = headers.get("cookie")?.to_str().ok()?;

    cookie.split(';').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        (name == SESSION_COOKIE).then(|| value.to_string())
    })
}
