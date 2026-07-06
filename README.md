# Workspace Manager Deployment

Gitea Code Spaces workspace manager. The manager runs as a local binary and creates code-server Docker containers for workspaces.

## Build Manager Binary

Build the workspace manager application:

```bash
cargo build --release
```

The compiled binary is generated at:

```text
target/release/workspace-manager
```

Optional Linux musl amd64 build:

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

The musl binary is generated at:

```text
target/x86_64-unknown-linux-musl/release/workspace-manager
```

## Build Workspace Image

Build the custom code-server image first:

```bash
cd /Users/coder/Desktop/workspace-manager/docker
docker build -t gitea-code-server:latest .
```

## Run Manager

Minimal startup command:

```bash
PORT=20081 \
WORKSPACE_MANAGER_DATA_DIR=./data \
GITEA_OAUTH_CLIENT_ID=your_client_id \
GITEA_OAUTH_CLIENT_SECRET=your_client_secret \
WORKSPACE_IMAGE=gitea-code-server:latest \
./workspace-manager
```

Using the compiled Linux musl amd64 binary:

```bash
PORT=20081 \
WORKSPACE_MANAGER_DATA_DIR=./data \
GITEA_OAUTH_CLIENT_ID=your_client_id \
GITEA_OAUTH_CLIENT_SECRET=your_client_secret \
WORKSPACE_IMAGE=gitea-code-server:latest \
/path/to/workspace-manager
```

## OAuth Callback

Configure the Gitea OAuth application callback URL as:

```text
http://your-manager-host:20081/auth/gitea/callback
```

Example:

```text
http://192.168.1.1:20081/auth/gitea/callback
```

## Common Environment Variables

Required:

```bash
PORT=20081
WORKSPACE_MANAGER_DATA_DIR=./data
GITEA_OAUTH_CLIENT_ID=your_client_id
GITEA_OAUTH_CLIENT_SECRET=your_client_secret
WORKSPACE_IMAGE=gitea-code-server:latest
```

Optional workspace port range:

```bash
WORKSPACE_PORT_START=30000
WORKSPACE_PORT_END=30999
```

Optional code-server auth modes:

```bash
# No password for workspace containers
CODE_SERVER_AUTH=none

# Fixed password for all workspace containers
WORKSPACE_CODE_SERVER_PASSWORD=your_password
```

If neither `CODE_SERVER_AUTH=none` nor `WORKSPACE_CODE_SERVER_PASSWORD` is set, the manager generates a random password per workspace and returns it in the workspace list.

Optional GitHub Authentication mode for workspace containers:

```bash
# Default: use the official code-server GitHub Authentication extension.
WORKSPACE_GITHUB_AUTHENTICATION_MODE=official

# Use the modified GitHub Authentication extension that can create sessions from GITHUB_TOKEN.
WORKSPACE_GITHUB_AUTHENTICATION_MODE=env-token
WORKSPACE_GITHUB_TOKEN=ghp_xxx
```

`WORKSPACE_GITHUB_TOKEN` is passed into workspace containers as `GITHUB_TOKEN` only when configured. The modified authentication extension is additive and only activates token-based silent sessions when `GITHUB_AUTHENTICATION_MODE=env-token`; otherwise the official behavior is preserved.

Optional shared code-server user data strategy:

> Prefer sharing explicit files with `WORKSPACE_SHARED_FILES` instead of the whole code-server data directory. GitHub Copilot Chat authentication is not reliable as shared filesystem state.

```bash
# Do not share code-server user data between containers. This is the default.
WORKSPACE_SHARED_DATA=none

# Share code-server user data between containers owned by the same login user.
WORKSPACE_SHARED_DATA=user

# Share code-server user data between all workspace containers.
# This can expose GitHub/Copilot sessions across users; use only in trusted single-user deployments.
WORKSPACE_SHARED_DATA=global
```

Shared code-server data is stored under `WORKSPACE_MANAGER_DATA_DIR`. If `WORKSPACE_SHARED_FILES` is empty, the selected shared directory is bind-mounted to `/home/coder/.local/share/code-server` for backward compatibility. If `WORKSPACE_SHARED_FILES` is set, only those comma-separated paths are shared. Paths can be files or directories. Relative paths are mounted under `/home/coder/.local/share/code-server`; absolute paths are mounted at that exact container path.

`WORKSPACE_SHARED_EXCLUDES` can hide comma-separated subdirectories from shared mounts by mounting workspace-private persistent directories over them. This keeps excluded data out of the shared directory while preserving it across restarts of the same workspace.

```text
WORKSPACE_SHARED_DATA=user
{WORKSPACE_MANAGER_DATA_DIR}/shared-code-server/users/{login}/

WORKSPACE_SHARED_DATA=global
{WORKSPACE_MANAGER_DATA_DIR}/shared-code-server/global/

WORKSPACE_SHARED_EXCLUDES=User/workspaceStorage
{WORKSPACE_MANAGER_DATA_DIR}/workspaces/{workspace_id}/private-code-server/User/workspaceStorage/
```

Example: share only custom chat model settings across all workspace containers:

```bash
WORKSPACE_SHARED_DATA=global
WORKSPACE_SHARED_FILES=User/chatLanguageModels.json
```

The same file can also be written as an absolute container path:

```bash
WORKSPACE_SHARED_DATA=global
WORKSPACE_SHARED_FILES=/home/coder/.local/share/code-server/User/chatLanguageModels.json
```

Example: share multiple paths per login user:

```bash
WORKSPACE_SHARED_DATA=user
WORKSPACE_SHARED_FILES=User/globalStorage,User/chatLanguageModels.json,User/settings.json
WORKSPACE_SHARED_EXCLUDES=User/globalStorage/github.copilot-chat/sessions
```

Example: share the whole code-server data directory, but keep workspace storage local to each container:

```bash
WORKSPACE_SHARED_DATA=global
WORKSPACE_SHARED_EXCLUDES=User/workspaceStorage
```

## Notes

- Workspace containers are named with the `gws-...` prefix and include user, repo, and workspace id information.
- Containers are created with `docker run -d`; stopping a container does not delete it.
- Deleting a workspace removes the container with `docker rm -f`.
- Workspace containers receive `WORKSPACE_TOKEN`; the custom image uses it to configure Git credential helper and disable workspace trust.

## Inject Into Gitea

Copy the frontend files to Gitea's public custom directory. Example:

```bash
mkdir -p /data/gitea/public/assets/codespaces
cp space-inject.js /data/gitea/public/assets/codespaces/space-inject.js
cp space-inject.css /data/gitea/public/assets/codespaces/space-inject.css
```

If your Gitea custom path is different, place the files under:

```text
<GITEA_CUSTOM>/public/assets/codespaces/
```

Then add the injection snippet to:

```text
<GITEA_CUSTOM>/templates/custom/footer.tmpl
```

Example `footer.tmpl`:

```html
<script>
  window.GITEA_CODE_SPACE_CONFIG = {
    workspaceManagerUrl: "http://192.168.1.1:20081"
  };
</script>
<link rel="stylesheet" href="/assets/codespaces/space-inject.css">
<script src="/assets/codespaces/space-inject.js" defer></script>
```

Restart Gitea after updating `footer.tmpl` or static assets.

For Docker deployments, mount the custom directory into the Gitea container and put the files in the mounted path. For example:

```text
/data/gitea/public/assets/codespaces/space-inject.js
/data/gitea/public/assets/codespaces/space-inject.css
/data/gitea/templates/custom/footer.tmpl
```

After deployment, open a repository page and click the normal clone button. The clone popup should show a `Code Spaces` tab.
