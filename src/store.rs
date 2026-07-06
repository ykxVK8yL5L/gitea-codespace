use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use tokio::sync::RwLock;

use crate::workspace::{CreateWorkspaceRequest, Workspace, WorkspaceQuery, WorkspaceStatus};

#[derive(Debug, Clone, Default)]
pub struct WorkspaceStore {
    inner: Arc<RwLock<HashMap<String, Workspace>>>,
    persist_path: Option<Arc<PathBuf>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedWorkspace {
    exists: bool,
    workspace_id: String,
    workspace_token: String,
    session_id: String,
    gitea_base_url: String,
    user_id: String,
    repo: String,
    branch: String,
    clone_url: Option<String>,
    status: WorkspaceStatus,
    url: Option<String>,
    #[serde(default)]
    container_ip: Option<String>,
    container_name: String,
    code_server_password: Option<String>,
    volume_name: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<Workspace> for PersistedWorkspace {
    fn from(workspace: Workspace) -> Self {
        Self {
            exists: workspace.exists,
            workspace_id: workspace.workspace_id,
            workspace_token: workspace.workspace_token,
            session_id: workspace.session_id,
            gitea_base_url: workspace.gitea_base_url,
            user_id: workspace.user_id,
            repo: workspace.repo,
            branch: workspace.branch,
            clone_url: workspace.clone_url,
            status: workspace.status,
            url: workspace.url,
            container_ip: workspace.container_ip,
            container_name: workspace.container_name,
            code_server_password: workspace.code_server_password,
            volume_name: workspace.volume_name,
            created_at: workspace.created_at,
            updated_at: workspace.updated_at,
        }
    }
}

impl From<PersistedWorkspace> for Workspace {
    fn from(workspace: PersistedWorkspace) -> Self {
        Self {
            exists: workspace.exists,
            workspace_id: workspace.workspace_id,
            workspace_token: workspace.workspace_token,
            session_id: workspace.session_id,
            gitea_base_url: workspace.gitea_base_url,
            user_id: workspace.user_id,
            repo: workspace.repo,
            branch: workspace.branch,
            clone_url: workspace.clone_url,
            status: workspace.status,
            url: workspace.url,
            container_ip: workspace.container_ip,
            container_name: workspace.container_name,
            code_server_password: workspace.code_server_password,
            volume_name: workspace.volume_name,
            created_at: workspace.created_at,
            updated_at: workspace.updated_at,
        }
    }
}

impl WorkspaceStore {
    pub fn new(data_dir: impl AsRef<Path>) -> anyhow::Result<Self> {
        let persist_path = data_dir.as_ref().join("workspaces.json");
        let workspaces = load_workspaces_from(&persist_path)?;
        Ok(Self {
            inner: Arc::new(RwLock::new(workspaces)),
            persist_path: Some(Arc::new(persist_path)),
        })
    }

    pub async fn create(
        &self,
        req: CreateWorkspaceRequest,
        user_id: String,
        session_id: String,
        gitea_base_url: String,
    ) -> Workspace {
        let workspace = Workspace::new_for_user(req, user_id, session_id, gitea_base_url);
        {
            self.inner
                .write()
                .await
                .insert(workspace.workspace_id.clone(), workspace.clone());
        }
        let _ = self.persist().await;
        workspace
    }

    pub async fn get(&self, workspace_id: &str) -> Option<Workspace> {
        self.inner.read().await.get(workspace_id).cloned()
    }

    pub async fn list(&self, query: &WorkspaceQuery) -> Vec<Workspace> {
        self.inner
            .read()
            .await
            .values()
            .filter(|workspace| workspace.matches_query(query))
            .cloned()
            .collect()
    }

    pub async fn get_by_workspace_token(&self, token: &str) -> Option<Workspace> {
        self.inner
            .read()
            .await
            .values()
            .find(|workspace| workspace.workspace_token == token)
            .cloned()
    }

    pub async fn mark(
        &self,
        workspace_id: &str,
        status: WorkspaceStatus,
        url: Option<String>,
    ) -> Option<Workspace> {
        let workspace = {
            let mut guard = self.inner.write().await;
            let workspace = guard.get_mut(workspace_id)?;
            workspace.mark(status, url);
            workspace.clone()
        };
        let _ = self.persist().await;
        Some(workspace)
    }

    pub async fn set_container_ip(
        &self,
        workspace_id: &str,
        container_ip: Option<String>,
    ) -> Option<Workspace> {
        let workspace = {
            let mut guard = self.inner.write().await;
            let workspace = guard.get_mut(workspace_id)?;
            workspace.set_container_ip(container_ip);
            workspace.clone()
        };
        let _ = self.persist().await;
        Some(workspace)
    }

    pub async fn set_code_server_password(
        &self,
        workspace_id: &str,
        password: Option<String>,
    ) -> Option<Workspace> {
        let workspace = {
            let mut guard = self.inner.write().await;
            let workspace = guard.get_mut(workspace_id)?;
            workspace.code_server_password = password;
            workspace.clone()
        };
        let _ = self.persist().await;
        Some(workspace)
    }

    pub async fn remove(&self, workspace_id: &str) -> Option<Workspace> {
        let removed = self.inner.write().await.remove(workspace_id);
        if removed.is_some() {
            let _ = self.persist().await;
        }
        removed
    }

    async fn persist(&self) -> anyhow::Result<()> {
        let Some(path) = self.persist_path.as_ref() else {
            return Ok(());
        };
        let workspaces = self.inner.read().await;
        let mut values = workspaces
            .values()
            .cloned()
            .map(PersistedWorkspace::from)
            .collect::<Vec<_>>();
        values.sort_by(|a, b| a.workspace_id.cmp(&b.workspace_id));
        let data = serde_json::to_vec_pretty(&values)?;
        std::fs::write(path.as_ref(), data)?;
        Ok(())
    }
}

fn load_workspaces_from(path: &Path) -> anyhow::Result<HashMap<String, Workspace>> {
    match std::fs::read(path) {
        Ok(data) => {
            let workspaces = serde_json::from_slice::<Vec<PersistedWorkspace>>(&data)?;
            Ok(workspaces
                .into_iter()
                .map(Workspace::from)
                .map(|workspace| (workspace.workspace_id.clone(), workspace))
                .collect())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_and_get_workspace() {
        let store = WorkspaceStore::default();
        let workspace = store
            .create(
                CreateWorkspaceRequest {
                    repo: "root/mycode".to_string(),
                    branch: Some("master".to_string()),
                    clone_url: None,
                },
                "root".to_string(),
                "session-1".to_string(),
                "http://gitea.local".to_string(),
            )
            .await;

        let fetched = store.get(&workspace.workspace_id).await.unwrap();
        assert_eq!(fetched.repo, "root/mycode");
        assert_eq!(fetched.branch, "master");
    }

    #[tokio::test]
    async fn filters_by_repo_branch_and_user() {
        let store = WorkspaceStore::default();
        store
            .create(
                CreateWorkspaceRequest {
                    repo: "root/mycode".to_string(),
                    branch: Some("master".to_string()),
                    clone_url: None,
                },
                "root".to_string(),
                "session-1".to_string(),
                "http://gitea.local".to_string(),
            )
            .await;

        let query = WorkspaceQuery {
            repo: Some("root/mycode".to_string()),
            branch: Some("master".to_string()),
            user_id: Some("root".to_string()),
        };

        assert_eq!(store.list(&query).await.len(), 1);

        let query = WorkspaceQuery {
            repo: Some("root/mycode".to_string()),
            branch: Some("dev".to_string()),
            user_id: Some("root".to_string()),
        };

        assert!(store.list(&query).await.is_empty());
    }
}
