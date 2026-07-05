use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub data_dir: PathBuf,
    pub auth: Option<AuthConfig>,
    pub workspace: WorkspaceConfig,
}

#[derive(Debug, Clone)]
pub struct WorkspaceConfig {
    pub image: String,
    pub port_start: u16,
    pub port_end: u16,
    pub code_server_auth: Option<String>,
    pub code_server_password: Option<String>,
    pub github_authentication_mode: Option<String>,
    pub github_token: Option<String>,
    pub shared_data: WorkspaceSharedData,
    pub shared_files: Vec<PathBuf>,
    pub shared_excludes: Vec<PathBuf>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceSharedData {
    None,
    User,
    Global,
}

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub public_url: Option<String>,
    pub client_id: String,
    pub client_secret: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let data_dir = std::env::var("WORKSPACE_MANAGER_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("data"));

        std::fs::create_dir_all(&data_dir)?;

        Ok(Self {
            data_dir,
            auth: AuthConfig::from_env(),
            workspace: WorkspaceConfig::from_env(),
        })
    }
}

impl WorkspaceConfig {
    fn from_env() -> Self {
        let port_start = read_u16("WORKSPACE_PORT_START").unwrap_or(30000);
        let mut port_end = read_u16("WORKSPACE_PORT_END").unwrap_or(30999);
        if port_end < port_start {
            port_end = port_start;
        }

        Self {
            image: read_non_empty("WORKSPACE_IMAGE")
                .unwrap_or_else(|| "gitea-code-server:latest".to_string()),
            port_start,
            port_end,
            code_server_auth: read_non_empty("CODE_SERVER_AUTH"),
            code_server_password: read_non_empty("WORKSPACE_CODE_SERVER_PASSWORD"),
            github_authentication_mode: read_non_empty("WORKSPACE_GITHUB_AUTHENTICATION_MODE"),
            github_token: read_non_empty("WORKSPACE_GITHUB_TOKEN"),
            shared_data: WorkspaceSharedData::from_env(),
            shared_files: read_path_list("WORKSPACE_SHARED_FILES"),
            shared_excludes: read_path_list("WORKSPACE_SHARED_EXCLUDES"),
        }
    }
}

impl WorkspaceSharedData {
    fn from_env() -> Self {
        match read_non_empty("WORKSPACE_SHARED_DATA")
            .unwrap_or_else(|| "none".to_string())
            .to_ascii_lowercase()
            .as_str()
        {
            "global" | "all" => Self::Global,
            "user" | "per-user" | "same-user" => Self::User,
            _ => Self::None,
        }
    }
}

impl AuthConfig {
    fn from_env() -> Option<Self> {
        Some(Self {
            public_url: read_non_empty("WORKSPACE_MANAGER_PUBLIC_URL"),
            client_id: read_non_empty("GITEA_OAUTH_CLIENT_ID")?,
            client_secret: read_non_empty("GITEA_OAUTH_CLIENT_SECRET")?,
        })
    }
}

fn read_non_empty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn read_u16(name: &str) -> Option<u16> {
    read_non_empty(name)?.parse().ok()
}

fn read_path_list(name: &str) -> Vec<PathBuf> {
    read_non_empty(name)
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default()
}
