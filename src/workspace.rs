use chrono::{DateTime, Utc};
use rand::distributions::{Alphanumeric, DistString};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::naming::{WorkspaceIdentity, WorkspaceNames};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceStatus {
    Creating,
    Running,
    Stopped,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub exists: bool,
    pub workspace_id: String,
    #[serde(skip)]
    pub workspace_token: String,
    #[serde(skip)]
    pub session_id: String,
    #[serde(skip)]
    pub gitea_base_url: String,
    pub user_id: String,
    pub repo: String,
    pub branch: String,
    pub clone_url: Option<String>,
    pub status: WorkspaceStatus,
    pub url: Option<String>,
    pub container_name: String,
    pub code_server_password: Option<String>,
    #[serde(skip)]
    pub volume_name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateWorkspaceRequest {
    pub repo: String,
    pub branch: Option<String>,
    pub clone_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkspaceQuery {
    pub repo: Option<String>,
    pub branch: Option<String>,
    pub user_id: Option<String>,
}

impl Workspace {
    pub fn new_for_user(
        req: CreateWorkspaceRequest,
        user_id: String,
        session_id: String,
        gitea_base_url: String,
    ) -> Self {
        let now = Utc::now();
        let workspace_id = format!("ws_{}", Uuid::new_v4().simple());
        let branch = req.branch.unwrap_or_else(|| "main".to_string());
        let identity = WorkspaceIdentity {
            workspace_id: workspace_id.clone(),
            user_id: user_id.clone(),
            repo: req.repo.clone(),
        };
        let WorkspaceNames {
            container_name,
            volume_name,
            ..
        } = identity.names();

        Self {
            exists: true,
            workspace_id,
            workspace_token: Alphanumeric.sample_string(&mut rand::thread_rng(), 48),
            session_id,
            gitea_base_url,
            user_id,
            repo: req.repo,
            branch,
            clone_url: req.clone_url,
            status: WorkspaceStatus::Creating,
            url: None,
            container_name,
            code_server_password: Some(Alphanumeric.sample_string(&mut rand::thread_rng(), 18)),
            volume_name,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn mark(&mut self, status: WorkspaceStatus, url: Option<String>) {
        self.status = status;
        if url.is_some() {
            self.url = url;
        }
        self.updated_at = Utc::now();
    }

    pub fn matches_query(&self, query: &WorkspaceQuery) -> bool {
        query.repo.as_ref().is_none_or(|repo| repo == &self.repo)
            && query
                .branch
                .as_ref()
                .is_none_or(|branch| branch == &self.branch)
            && query
                .user_id
                .as_ref()
                .is_none_or(|user_id| user_id == &self.user_id)
    }
}
