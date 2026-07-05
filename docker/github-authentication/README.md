# GitHub Authentication env-token Variant

This directory contains the code-server GitHub Authentication override used by workspace containers.

The goal is narrow: keep the official VS Code `github-authentication` behavior as the default, and add an optional `env-token` mode that can create GitHub authentication sessions from container environment tokens.

## Runtime behavior

Default mode is official:

```bash
GITHUB_AUTHENTICATION_MODE=official
```

Env-token mode is enabled only when the container has:

```bash
GITHUB_AUTHENTICATION_MODE=env-token
GITHUB_TOKEN=ghp_xxx
```

`WORKSPACE_GITHUB_TOKEN` is also accepted as a fallback token name because the workspace manager may pass that value through.

When enabled, the env-token provider creates GitHub Authentication sessions from the configured token for the scopes requested by VS Code extensions. If mode is not `env-token`, or no token is present, the official behavior is preserved.

After env-token sign-in succeeds, the source override runs github.copilot.refreshToken once to refresh Copilot status. It intentionally does not run permissive sign-in commands.

## Files to maintain

`upstream/`

Vendored copy of VS Code `extensions/github-authentication`. The env-token changes are maintained directly in this source tree. Keep it in the repo after a known-good update so Docker builds are reproducible.

`build.sh`

Copies `upstream/` to `build/`, installs build dependencies, and writes `dist/github-authentication-env-token/`.

`scripts/bundle-node-extension.js`

Bundles the vendored extension into `dist/browser/extension.js` and rewrites `package.json` so code-server loads the browser bundle.

`../entrypoint.sh`

At container startup, replaces the official GitHub Authentication extension with `/opt/github-authentication/env-token` only when `GITHUB_AUTHENTICATION_MODE=env-token`.

`../Dockerfile`

Builds the env-token variant in the `github-authentication-builder` stage and copies it into `/opt/github-authentication/env-token` in the runtime image.

## Source responsibilities

The vendored source differs from official upstream in these areas.

`package.json`

- Adds activation for startup and the manual commands.
- Adds command palette entries for `github-authentication.signIn` and `github-authentication.signInWithPAT`.

`src/extension.ts`

- Stores `new GitHubAuthenticationProvider(...)` in a local `githubAuthProvider` variable.
- On startup, when `GITHUB_AUTHENTICATION_MODE=env-token` and a token exists, creates a `user:email` session through `githubAuthProvider.createSession(['user:email'])`.
- Registers `github-authentication.signIn` for normal VS Code auth flow.
- Registers `github-authentication.signInWithPAT` for manually entering a PAT.
- Runs github.copilot.refreshToken once after env-token sign-in succeeds, but does not run permissive sign-in commands.

`src/github.ts`

- Adds constants for `GITHUB_AUTHENTICATION_MODE`, `GITHUB_TOKEN`, `WORKSPACE_GITHUB_TOKEN`, and the PAT secret key.
- Replaces the `getSessions` session source with `ensureConfiguredTokenSession(...)`.
- Reads token from environment first, then from the stored PAT secret.
- Creates missing scoped sessions lazily when extensions request scopes.
- Stores token-created sessions and fires the normal session-change event.
- Exposes `createSessionWithPAT(...)` for the manual PAT command.

## Upgrade checklist

Use this checklist when replacing `upstream/` with a newer VS Code source copy.

1. Update `upstream/`, `VSCODE_COMMIT`, and `UPSTREAM_INFO.json` together.
2. Port the source responsibilities listed above into the new upstream files.
3. Build the extension locally before building the image.
4. If a previous patch file is used as a reference, do not leave it wired into `build.sh`.
5. Confirm the bundle still emits `dist/browser/extension.js` and that `package.json` points both `main` and `browser` at that file.
6. Confirm `entrypoint.sh` still prints `GitHub Authentication mode: env-token` and replaces only the GitHub Authentication extension.
7. Run a container with `GITHUB_AUTHENTICATION_MODE=env-token` and a token, then verify GitHub sign-in state.
8. Verify manual `GitHub: Sign In with PAT` still works as a fallback.

Do not restore the old silent fallback that creates an empty env-token build directory. If the modified extension fails to build, the Docker build should fail clearly.

## Build commands

Build the extension locally when Node dependencies are available:

```bash
./docker/github-authentication/build.sh
```

Output:

```text
docker/github-authentication/dist/github-authentication-env-token/
```

The Docker image build also runs the same builder flow.

## Common failures

`GitHub Authentication env-token build is missing; falling back to official`

The runtime image does not contain a valid `/opt/github-authentication/env-token/package.json`. Treat this as a build or copy problem.

GitHub account is signed in but Copilot still needs interaction

The GitHub Authentication session exists, but Copilot has not refreshed or accepted it yet. This source override only calls the no-UI github.copilot.refreshToken status refresh; avoid commands that force permissive sign-in UI.
