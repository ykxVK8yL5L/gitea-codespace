# Chat Language Model Loader

A minimal VS Code/code-server extension that registers OpenAI-compatible chat models from a local JSON file using the VS Code Language Model Chat Provider API.

This extension is modeled after OAI-style providers: it does not edit `chatLanguageModels.json`. Models are registered through `vscode.lm.registerLanguageModelChatProvider`, and API keys are stored in VS Code SecretStorage.

## Requirements

- VS Code/code-server build with `vscode.lm.registerLanguageModelChatProvider` support.
- Proposed API support for `chatProvider`.
- An OpenAI-compatible chat completions endpoint.

If the runtime does not support the provider API, the extension shows:

```text
vscode.lm.registerLanguageModelChatProvider is unavailable in this code-server/VS Code build.
```

## Model File

Default path:

```text
/root/models.json
```

Example:

```json
[
	{
		"url": "https://abc.com/api/v1",
		"apikey": "sk-xxx",
		"model": "glm-5.2",
		"alias": "GLM 5.2",
		"type": "completions"
	}
]
```

Fields:

- `url`: Base URL for the OpenAI-compatible API. The extension calls `${url}/chat/completions` for `type: "completions"`.
- `apikey`: API key. It is imported into VS Code SecretStorage when models are loaded.
- `model`: Upstream model ID sent in the request body.
- `alias` optional: Display name shown in the model picker. Requests still use `model`.
- `type`: Currently `completions` is implemented. `responses` is accepted in the file but intentionally returns a not-implemented error in this minimal version.
- `maxInputTokens` optional, defaults to `128000`.
- `maxOutputTokens` optional, defaults to `16000`.

## Deduplication

The extension deduplicates models by this fingerprint:

```text
type + normalized url + apikey + model + alias
```

`url` is normalized by removing trailing slashes. If all values match, only one model is loaded. The generated model ID is stable, for example:

```text
local-oai-8f3a21c9
```

## Persistence

Model loading is manual and persistent:

- Run `Load Local OAI Models From File` once to load `/root/models.json`.
- Model metadata is stored in `globalState` without the raw API key.
- API keys are stored in SecretStorage.
- On code-server restart, the extension restores persisted models and does not reread `/root/models.json`.
- To change models, edit `/root/models.json` and run `Load Local OAI Models From File` again.

## Settings

```json
{
	"chatLanguageModelLoader.sourceModelsPath": "/root/models.json",
	"chatLanguageModelLoader.loadOnStartup": false
}
```

`loadOnStartup` defaults to `false`. If set to `true`, the extension will read the source file on startup only when no persisted models exist.

## Commands

Open the Command Palette and run:

```text
Chat Model Loader: Load Local OAI Models From File
```

After loading, reload the window or restart code-server if the model picker does not refresh immediately.

## Package as VSIX

This project has no build step. It is a plain JavaScript extension.

### Option 1: package with vsce

Install `vsce` if needed:

```bash
npm install -g @vscode/vsce
```

Package:

```bash
cd chat-language-model-loader
vsce package
```

This creates a `.vsix` file in the project directory.

### Option 2: package manually with zip

A VSIX is a zip file with this layout:

```text
[Content_Types].xml
extension.vsixmanifest
extension/package.json
extension/extension.js
extension/README.md
```

The project directory itself is not directly a VSIX; package it with `vsce` unless you specifically need manual packaging.

## Install in code-server

```bash
code-server --install-extension /path/to/chat-language-model-loader-0.1.4.vsix --force
```

Then restart code-server.

## Troubleshooting

Check extension logs:

```text
Developer: Show Logs... -> Extension Host
```

Or open the Output panel and select:

```text
Chat Model Loader
```

Common issues:

- Model picker does not refresh: reload the VS Code/code-server window.
- `provideTokenCount is not a function`: install version `0.1.1` or newer.
- Provider API unavailable: the current code-server build does not support the proposed language model provider API.
- API request fails: verify `url`, `apikey`, and `model` in `/root/models.json`.
