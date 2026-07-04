use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use rand::distributions::{Alphanumeric, DistString};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct AuthStore {
    data_dir: PathBuf,
    inner: Arc<RwLock<AuthState>>,
}

#[derive(Debug, Default)]
struct AuthState {
    oauth_states: HashMap<String, PendingOAuth>,
    sessions: HashMap<String, UserSession>,
    token_checks: HashMap<String, Instant>,
}

#[derive(Debug, Clone)]
pub struct PendingOAuth {
    pub gitea_base_url: String,
    pub redirect_uri: String,
    pub return_to: Option<String>,
    pub popup: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSession {
    pub gitea_base_url: String,
    pub access_token: String,
    pub user: GiteaUser,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GiteaUser {
    pub id: i64,
    pub login: String,
    pub full_name: Option<String>,
    pub email: Option<String>,
}

impl AuthStore {
    pub fn new(data_dir: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let data_dir = data_dir.into();
        let sessions_dir = data_dir.join("sessions");
        std::fs::create_dir_all(&sessions_dir)?;

        let sessions = load_sessions_from(&sessions_dir)?;

        Ok(Self {
            data_dir,
            inner: Arc::new(RwLock::new(AuthState {
                oauth_states: HashMap::new(),
                sessions,
                token_checks: HashMap::new(),
            })),
        })
    }

    pub async fn create_oauth_state(&self, pending: PendingOAuth) -> String {
        let state = random_token(32);
        self.inner
            .write()
            .await
            .oauth_states
            .insert(state.clone(), pending);
        state
    }

    pub async fn take_oauth_state(&self, state: &str) -> Option<PendingOAuth> {
        self.inner.write().await.oauth_states.remove(state)
    }

    pub async fn create_session(&self, session: UserSession) -> anyhow::Result<String> {
        let session_id = random_token(48);
        self.write_session(&session_id, &session)?;
        self.inner
            .write()
            .await
            .sessions
            .insert(session_id.clone(), session);
        Ok(session_id)
    }

    pub async fn get_session(&self, session_id: &str) -> Option<UserSession> {
        self.inner.read().await.sessions.get(session_id).cloned()
    }

    pub async fn remove_session(&self, session_id: &str) -> anyhow::Result<()> {
        {
            let mut inner = self.inner.write().await;
            inner.sessions.remove(session_id);
            inner.token_checks.remove(session_id);
        }

        let path = self.session_path(session_id);
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    pub async fn token_check_is_fresh(&self, session_id: &str, ttl: Duration) -> bool {
        self.inner
            .read()
            .await
            .token_checks
            .get(session_id)
            .is_some_and(|checked_at| checked_at.elapsed() < ttl)
    }

    pub async fn mark_token_checked(&self, session_id: &str) {
        self.inner
            .write()
            .await
            .token_checks
            .insert(session_id.to_string(), Instant::now());
    }

    fn sessions_dir(&self) -> PathBuf {
        self.data_dir.join("sessions")
    }

    fn session_path(&self, session_id: &str) -> PathBuf {
        self.sessions_dir().join(format!("{session_id}.json"))
    }

    fn write_session(&self, session_id: &str, session: &UserSession) -> anyhow::Result<()> {
        let path = self.session_path(session_id);
        let data = serde_json::to_vec_pretty(session)?;
        write_secret_file(&path, &data)
    }
}

fn load_sessions_from(sessions_dir: &Path) -> anyhow::Result<HashMap<String, UserSession>> {
    let mut sessions = HashMap::new();

    for entry in std::fs::read_dir(sessions_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }

        let Some(session_id) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        let data = std::fs::read(&path)
            .with_context(|| format!("failed to read session {}", path.display()))?;
        let session = serde_json::from_slice::<UserSession>(&data)
            .with_context(|| format!("failed to parse session {}", path.display()))?;
        sessions.insert(session_id.to_string(), session);
    }

    Ok(sessions)
}

fn random_token(len: usize) -> String {
    Alphanumeric.sample_string(&mut rand::thread_rng(), len)
}

#[cfg(unix)]
fn write_secret_file(path: &Path, data: &[u8]) -> anyhow::Result<()> {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(data)?;
    Ok(())
}

#[cfg(not(unix))]
fn write_secret_file(path: &Path, data: &[u8]) -> anyhow::Result<()> {
    std::fs::write(path, data)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn oauth_state_is_single_use() {
        let store = AuthStore::new(std::env::temp_dir().join(random_token(12))).unwrap();
        let state = store
            .create_oauth_state(PendingOAuth {
                gitea_base_url: "http://gitea.local".to_string(),
                redirect_uri: "http://manager.local/auth/gitea/callback".to_string(),
                return_to: Some("/root/repo".to_string()),
                popup: false,
            })
            .await;

        assert!(store.take_oauth_state(&state).await.is_some());
        assert!(store.take_oauth_state(&state).await.is_none());
    }

    #[tokio::test]
    async fn session_is_persisted_and_reloaded() {
        let data_dir = std::env::temp_dir().join(random_token(12));
        let store = AuthStore::new(&data_dir).unwrap();
        let session_id = store
            .create_session(UserSession {
                gitea_base_url: "http://gitea.local".to_string(),
                access_token: "token".to_string(),
                user: GiteaUser {
                    id: 1,
                    login: "alice".to_string(),
                    full_name: None,
                    email: None,
                },
            })
            .await
            .unwrap();

        let reloaded = AuthStore::new(&data_dir).unwrap();
        let session = reloaded.get_session(&session_id).await.unwrap();
        assert_eq!(session.user.login, "alice");
        assert_eq!(session.access_token, "token");
    }
}
