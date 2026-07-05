# GitHub Authentication env-token Variant

This directory vendors the official VS Code `extensions/github-authentication` source and applies a small additive patch.

The default workspace behavior remains the official code-server GitHub Authentication extension. The modified extension is selected only when the workspace container has:

```bash
GITHUB_AUTHENTICATION_MODE=env-token
GITHUB_TOKEN=ghp_xxx
```

When enabled, the modified provider creates GitHub Authentication sessions from `GITHUB_TOKEN` for the exact scopes requested by VS Code/Copilot. If the mode is not `env-token`, or if `GITHUB_TOKEN` is empty, official login behavior is preserved.

## Update upstream source

```bash
./docker/github-authentication/update-upstream.sh
```

Optional variables:

```bash
VSCODE_REPO_URL=https://github.com/microsoft/vscode.git
VSCODE_REF=main
```

The script writes:

- `upstream/` official source copy
- `VSCODE_COMMIT`
- `UPSTREAM_INFO.json`

## Build modified extension

```bash
./docker/github-authentication/build.sh
```

Output:

```text
docker/github-authentication/dist/github-authentication-env-token/
```

The Docker image build also runs `build.sh`. If the build fails, the image keeps the official extension available and the entrypoint falls back to official mode.

## Patch policy

Patch files live under `patches/`. Keep patches small and additive. Do not delete the official extension from the image.
