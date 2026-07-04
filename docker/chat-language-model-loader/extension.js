const fs = require("fs/promises");
const vscode = require("vscode");

const EXTENSION_ID = "chatLanguageModelLoader";
const PROVIDER_VENDOR = "local-oai";
const DEFAULT_SOURCE_MODELS_PATH = "/root/models.json";
const SECRET_PREFIX = "chatLanguageModelLoader.apiKey";
const MODELS_STATE_KEY = "chatLanguageModelLoader.models";

let loadedModels = [];
let output;
let secrets;
let extensionContext;

function activate(context) {
	output = vscode.window.createOutputChannel("Chat Model Loader");
	secrets = context.secrets;
	extensionContext = context;

	context.subscriptions.push(output);
	context.subscriptions.push(
		vscode.commands.registerCommand(`${EXTENSION_ID}.reloadModels`, async () => {
			await reloadModels({ showSuccess: true });
		})
	);

	if (!vscode.lm || typeof vscode.lm.registerLanguageModelChatProvider !== "function") {
		const message = "vscode.lm.registerLanguageModelChatProvider is unavailable in this code-server/VS Code build.";
		output.appendLine(message);
		vscode.window.showWarningMessage(message);
		return;
	}

	context.subscriptions.push(vscode.lm.registerLanguageModelChatProvider(PROVIDER_VENDOR, new LocalOaiProvider()));

	restorePersistedModels();

	const config = vscode.workspace.getConfiguration(EXTENSION_ID);
	if (config.get("loadOnStartup", true) && loadedModels.length === 0) {
		reloadModels({ showSuccess: false }).catch((error) => {
			const message = `Local OAI model load failed: ${getErrorMessage(error)}`;
			output.appendLine(message);
		});
	}
}

function deactivate() {}

class LocalOaiProvider {
	async provideLanguageModelChatInformation(_options, _token) {
		return loadedModels.map((model) => ({
			id: model.id,
			name: model.model,
			detail: "local-oai",
			tooltip: `${model.type} ${model.url}`,
			family: "oai-compatible",
			version: "1.0.0",
			maxInputTokens: model.maxInputTokens ?? 128000,
			maxOutputTokens: model.maxOutputTokens ?? 16000,
			isUserSelectable: true,
			capabilities: {
				toolCalling: true,
				imageInput: true,
			},
		}));
	}

	async provideTokenCount(_model, text, _token) {
		const value = typeof text === "string" ? text : contentToString(text?.content);
		return Math.max(1, Math.ceil(value.length / 4));
	}

	async provideLanguageModelChatResponse(modelInfo, messages, options, progress, token) {
		const model = loadedModels.find((item) => item.id === modelInfo.id);
		if (!model) {
			throw new Error(`Model not found: ${modelInfo.id}`);
		}

		const apiKey = await secrets.get(secretKeyFor(model));
		if (!apiKey) {
			throw new Error(`API key not found for ${model.model}. Reload models first.`);
		}

		if (model.type === "responses") {
			throw new Error("responses type is not implemented in the minimal provider yet. Use type=completions for the first test.");
		}

		const body = {
			model: model.model,
			messages: convertMessages(messages),
			stream: true,
		};

		if (typeof options?.temperature === "number") {
			body.temperature = options.temperature;
		}

		const url = `${model.url.replace(/\/+$/, "")}/chat/completions`;
		output.appendLine(`request ${model.model} -> ${url}`);
		const response = await fetch(url, {
			method: "POST",
			headers: {
				"Content-Type": "application/json",
				"Authorization": `Bearer ${apiKey}`,
			},
			body: JSON.stringify(body),
			signal: tokenToSignal(token),
		});

		if (!response.ok) {
			const text = await response.text().catch(() => "");
			throw new Error(`Local OAI request failed: ${response.status} ${response.statusText}${text ? `\n${text}` : ""}`);
		}

		if (!response.body) {
			throw new Error("Local OAI response has no body.");
		}

		await readOpenAIStream(response.body, progress);
	}
}

async function reloadModels(options) {
	const config = vscode.workspace.getConfiguration(EXTENSION_ID);
	const sourceModelsPath = config.get("sourceModelsPath", DEFAULT_SOURCE_MODELS_PATH).trim();
	const rawModels = await readJson(sourceModelsPath);
	if (!Array.isArray(rawModels)) {
		throw new Error("Source model JSON must be an array.");
	}

	const nextModels = dedupeModels(rawModels.map(normalizeSourceModel));
	for (const model of nextModels) {
		await secrets.store(secretKeyFor(model), model.apikey);
	}
	loadedModels = nextModels.map(withoutApiKey);
	await extensionContext.globalState.update(MODELS_STATE_KEY, loadedModels);
	output.appendLine(`loaded ${loadedModels.length} model(s) from ${sourceModelsPath}`);
	if (options.showSuccess) {
		vscode.window.showInformationMessage(`Loaded ${loadedModels.length} local OAI model(s). Reload window if the model picker does not refresh.`);
	}
}

function restorePersistedModels() {
	const persistedModels = extensionContext.globalState.get(MODELS_STATE_KEY, []);
	if (Array.isArray(persistedModels)) {
		loadedModels = persistedModels.filter((model) => model && typeof model.id === "string");
	}
	output.appendLine(`restored ${loadedModels.length} persisted model(s)`);
}

function withoutApiKey(model) {
	const { apikey, ...safeModel } = model;
	return safeModel;
}

async function readJson(filePath) {
	const raw = await fs.readFile(filePath, "utf8");
	return JSON.parse(raw);
}

function normalizeSourceModel(input, index) {
	if (!input || typeof input !== "object") {
		throw new Error(`Model at index ${index} must be an object.`);
	}
	const url = readRequiredString(input, "url", index);
	const apikey = readRequiredString(input, "apikey", index);
	const model = readRequiredString(input, "model", index);
	const type = readRequiredString(input, "type", index);
	if (type !== "completions" && type !== "responses") {
		throw new Error(`Model at index ${index} has unsupported type '${type}'. Use completions or responses.`);
	}
	const normalizedUrl = normalizeUrl(url);
	const fingerprint = `${type}:${normalizedUrl}:${apikey}:${model}`;
	return {
		id: `local-oai-${hashString(fingerprint)}`,
		fingerprint,
		url: normalizedUrl,
		apikey,
		model,
		type,
		maxInputTokens: numberOrDefault(input.maxInputTokens, 128000),
		maxOutputTokens: numberOrDefault(input.maxOutputTokens, 16000),
	};
}

function dedupeModels(models) {
	const byFingerprint = new Map();
	for (const model of models) {
		if (!byFingerprint.has(model.fingerprint)) {
			byFingerprint.set(model.fingerprint, model);
		}
	}
	return Array.from(byFingerprint.values());
}

function normalizeUrl(value) {
	return value.replace(/\/+$/, "");
}

function hashString(value) {
	let hash = 2166136261;
	for (let i = 0; i < value.length; i++) {
		hash ^= value.charCodeAt(i);
		hash = Math.imul(hash, 16777619);
	}
	return (hash >>> 0).toString(16).padStart(8, "0");
}

function readRequiredString(input, key, index) {
	const value = input[key];
	if (typeof value !== "string" || value.trim() === "") {
		throw new Error(`Model at index ${index} is missing string field '${key}'.`);
	}
	return value.trim();
}

function numberOrDefault(value, fallback) {
	return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

function secretKeyFor(model) {
	return `${SECRET_PREFIX}.${model.id}`;
}

function convertMessages(messages) {
	return messages.map((message) => ({
		role: roleToString(message.role),
		content: contentToString(message.content),
	}));
}

function roleToString(role) {
	const value = Number(role);
	if (value === Number(vscode.LanguageModelChatMessageRole.Assistant)) {
		return "assistant";
	}
	if (value === Number(vscode.LanguageModelChatMessageRole.User)) {
		return "user";
	}
	return "system";
}

function contentToString(content) {
	if (typeof content === "string") {
		return content;
	}
	if (!Array.isArray(content)) {
		return "";
	}
	return content.map((part) => {
		if (part instanceof vscode.LanguageModelTextPart) {
			return part.value;
		}
		if (part && typeof part.value === "string") {
			return part.value;
		}
		return "";
	}).join("");
}

function tokenToSignal(token) {
	const controller = new AbortController();
	if (token?.isCancellationRequested) {
		controller.abort();
	}
	if (token?.onCancellationRequested) {
		token.onCancellationRequested(() => controller.abort());
	}
	return controller.signal;
}

async function readOpenAIStream(body, progress) {
	const decoder = new TextDecoder();
	let buffer = "";
	for await (const chunk of body) {
		buffer += decoder.decode(chunk, { stream: true });
		let splitIndex;
		while ((splitIndex = buffer.indexOf("\n\n")) !== -1) {
			const event = buffer.slice(0, splitIndex);
			buffer = buffer.slice(splitIndex + 2);
			handleSseEvent(event, progress);
		}
	}
	if (buffer.trim()) {
		handleSseEvent(buffer, progress);
	}
}

function handleSseEvent(event, progress) {
	const lines = event.split(/\r?\n/);
	for (const line of lines) {
		const trimmed = line.trim();
		if (!trimmed.startsWith("data:")) {
			continue;
		}
		const data = trimmed.slice("data:".length).trim();
		if (!data || data === "[DONE]") {
			continue;
		}
		try {
			const parsed = JSON.parse(data);
			const text = parsed.choices?.[0]?.delta?.content ?? parsed.choices?.[0]?.message?.content ?? "";
			if (text) {
				progress.report(new vscode.LanguageModelTextPart(text));
			}
		} catch (error) {
			output.appendLine(`stream parse failed: ${getErrorMessage(error)}`);
		}
	}
}

function getErrorMessage(error) {
	return error instanceof Error ? error.message : String(error);
}

module.exports = {
	activate,
	deactivate,
};
