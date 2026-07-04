use serde::{Deserialize, Serialize};

pub const MANAGED_BY_LABEL: &str = "space.manager/managed-by";
pub const MANAGED_BY_VALUE: &str = "gitea-workspace-manager";
pub const WORKSPACE_ID_LABEL: &str = "space.manager/workspace-id";
pub const REPO_LABEL: &str = "space.manager/repo";
pub const USER_ID_LABEL: &str = "space.manager/user-id";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceNames {
    pub workspace_id: String,
    pub container_name: String,
    pub volume_name: String,
    pub network_name: String,
}

#[derive(Debug, Clone)]
pub struct WorkspaceIdentity {
    pub workspace_id: String,
    pub user_id: String,
    pub repo: String,
}

impl WorkspaceIdentity {
    pub fn names(&self) -> WorkspaceNames {
        let user = slug_component(&self.user_id, 24);
        let repo = repo_slug(&self.repo, 48);
        let id = slug_component(&self.workspace_id, 24);
        let base = format!("gws-{user}-{repo}-{id}");

        WorkspaceNames {
            workspace_id: self.workspace_id.clone(),
            container_name: truncate_component(&base, 120),
            volume_name: truncate_component(&format!("{base}-data"), 120),
            network_name: "gws-network".to_string(),
        }
    }

    pub fn docker_labels(&self) -> Vec<(String, String)> {
        vec![
            (MANAGED_BY_LABEL.to_string(), MANAGED_BY_VALUE.to_string()),
            (WORKSPACE_ID_LABEL.to_string(), self.workspace_id.clone()),
            (USER_ID_LABEL.to_string(), self.user_id.clone()),
            (REPO_LABEL.to_string(), self.repo.clone()),
        ]
    }
}

pub fn is_managed_workspace(
    labels: &std::collections::HashMap<String, String>,
    workspace_id: &str,
) -> bool {
    labels
        .get(MANAGED_BY_LABEL)
        .is_some_and(|value| value == MANAGED_BY_VALUE)
        && labels
            .get(WORKSPACE_ID_LABEL)
            .is_some_and(|value| value == workspace_id)
}

fn repo_slug(repo: &str, max_len: usize) -> String {
    let repo = repo.replace('/', "-");
    slug_component(&repo, max_len)
}

pub fn slug_component(value: &str, max_len: usize) -> String {
    let mut out = String::with_capacity(value.len());
    let mut last_dash = false;

    for ch in value.chars() {
        let normalized = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else {
            '-'
        };

        if normalized == '-' {
            if !last_dash && !out.is_empty() {
                out.push('-');
            }
            last_dash = true;
        } else {
            out.push(normalized);
            last_dash = false;
        }
    }

    let trimmed = out.trim_matches('-');
    let fallback = if trimmed.is_empty() {
        "unknown"
    } else {
        trimmed
    };
    truncate_component(fallback, max_len)
}

fn truncate_component(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        return value.to_string();
    }

    value.chars().take(max_len).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn names_are_human_readable_and_stable() {
        let identity = WorkspaceIdentity {
            workspace_id: "ws_123ABC".to_string(),
            user_id: "Alice.Root".to_string(),
            repo: "root/mycode".to_string(),
        };

        let names = identity.names();

        assert_eq!(names.container_name, "gws-alice-root-root-mycode-ws-123abc");
        assert_eq!(
            names.volume_name,
            "gws-alice-root-root-mycode-ws-123abc-data"
        );
        assert_eq!(names.network_name, "gws-network");
    }

    #[test]
    fn managed_check_requires_service_label_and_workspace_id() {
        let mut labels = HashMap::new();
        labels.insert(MANAGED_BY_LABEL.to_string(), MANAGED_BY_VALUE.to_string());
        labels.insert(WORKSPACE_ID_LABEL.to_string(), "ws_123".to_string());

        assert!(is_managed_workspace(&labels, "ws_123"));
        assert!(!is_managed_workspace(&labels, "ws_456"));
    }
}
