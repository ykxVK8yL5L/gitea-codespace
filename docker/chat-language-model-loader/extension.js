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
				supportsToolCalling: true,
				agentMode: true,
				imageInput: true,
				supportsImageToText: true,
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

		const toolConfig = convertTools(options);
		if (toolConfig.tools.length > 0) {
			body.tools = toolConfig.tools;
			body.tool_choice = toolConfig.tool_choice;
		}

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
	const converted = [];
	for (const message of messages) {
		const role = roleToString(message.role);
		const textParts = [];
		const toolCalls = [];
		const toolResults = [];

		for (const part of normalizeContentParts(message.content)) {
			if (part instanceof vscode.LanguageModelTextPart) {
				textParts.push(part.value);
			} else if (part instanceof vscode.LanguageModelToolCallPart) {
				let args = "{}";
				try {
					args = JSON.stringify(part.input ?? {});
				} catch {}
				toolCalls.push({
					id: part.callId || `call_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
					type: "function",
					function: { name: part.name, arguments: args },
				});
			} else if (isToolResultPart(part)) {
				toolResults.push({
					callId: part.callId || "",
					content: collectToolResultText(part),
				});
			} else if (part && typeof part.value === "string") {
				textParts.push(part.value);
			}
		}

		const joinedText = textParts.join("").trim();

		if (role === "assistant") {
			const assistantMessage = { role: "assistant" };
			if (joinedText) {
				assistantMessage.content = joinedText;
			}
			if (toolCalls.length > 0) {
				assistantMessage.tool_calls = toolCalls;
			}
			if (assistantMessage.content || assistantMessage.tool_calls) {
				converted.push(assistantMessage);
			}
		}

		for (const result of toolResults) {
			converted.push({ role: "tool", tool_call_id: result.callId, content: result.content || "" });
		}

		if (role === "user" && joinedText) {
			converted.push({ role, content: joinedText });
		}

		if (role === "system" && joinedText) {
			converted.push({ role, content: joinedText });
		}
	}
	return converted;
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

function normalizeContentParts(content) {
	if (typeof content === "string") {
		return [new vscode.LanguageModelTextPart(content)];
	}
	return Array.isArray(content) ? content : [];
}

function isToolResultPart(part) {
	return part && typeof part === "object" && typeof part.callId === "string" && "content" in part;
}

function collectToolResultText(part) {
	let text = "";
	for (const item of part.content || []) {
		if (item instanceof vscode.LanguageModelTextPart) {
			text += item.value;
		} else if (typeof item === "string") {
			text += item;
		} else if (item instanceof vscode.LanguageModelDataPart && item.mimeType === "cache_control") {
			// ignored to match oai-compatible-copilot
		} else {
			try {
				text += JSON.stringify(item);
			} catch {}
		}
	}
	return text;
}

function tryParseJSONObject(text) {
	try {
		if (!text || !/[{]/.test(text)) {
			return { ok: false };
		}
		const value = JSON.parse(text);
		if (value && typeof value === "object" && !Array.isArray(value)) {
			return { ok: true, value };
		}
		return { ok: false };
	} catch {
		return { ok: false };
	}
}

function convertTools(options) {
	const tools = Array.isArray(options?.tools) ? options.tools : [];
	const toolDefs = tools.map((tool) => ({
		type: "function",
		function: {
			name: tool.name,
			description: typeof tool.description === "string" ? tool.description : "",
			parameters: tool.inputSchema ?? { type: "object", properties: {} },
		},
	}));

	let tool_choice = "auto";
	if (options?.toolMode === vscode.LanguageModelChatToolMode?.Required) {
		if (tools.length !== 1) {
			throw new Error("LanguageModelChatToolMode.Required is not supported with more than one tool");
		}
		tool_choice = { type: "function", function: { name: tools[0].name } };
	}

	return { tools: toolDefs, tool_choice };
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
	const toolCallBuffers = new Map();
	const completedToolCallIndices = new Set();
	const state = {
		hasEmittedAssistantText: false,
		emittedBeginToolCallsHint: false,
	};
	for await (const chunk of body) {
		buffer += decoder.decode(chunk, { stream: true });
		let splitIndex;
		while ((splitIndex = buffer.indexOf("\n\n")) !== -1) {
			const event = buffer.slice(0, splitIndex);
			buffer = buffer.slice(splitIndex + 2);
			handleSseEvent(event, progress, toolCallBuffers, completedToolCallIndices, state);
		}
	}
	if (buffer.trim()) {
		handleSseEvent(buffer, progress, toolCallBuffers, completedToolCallIndices, state);
	}
	flushToolCallBuffers(progress, toolCallBuffers, completedToolCallIndices, false);
}

function handleSseEvent(event, progress, toolCallBuffers, completedToolCallIndices, state) {
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
			const choice = parsed.choices?.[0] ?? {};
			const delta = choice.delta ?? choice.message ?? {};
			const text = delta.content ?? "";
			if (text) {
				progress.report(new vscode.LanguageModelTextPart(text));
				state.hasEmittedAssistantText = true;
			}
			if (Array.isArray(delta.tool_calls)) {
				if (!state.emittedBeginToolCallsHint && state.hasEmittedAssistantText && delta.tool_calls.length > 0) {
					progress.report(new vscode.LanguageModelTextPart(" "));
					state.emittedBeginToolCallsHint = true;
				}
				for (const toolCall of delta.tool_calls) {
					bufferToolCall(toolCall, toolCallBuffers, completedToolCallIndices);
					tryEmitBufferedToolCall(toolCall, progress, toolCallBuffers, completedToolCallIndices);
				}
			}
			if (choice.finish_reason === "tool_calls" || choice.finish_reason === "stop") {
				flushToolCallBuffers(progress, toolCallBuffers, completedToolCallIndices, true);
			}
		} catch (error) {
			output.appendLine(`stream parse failed: ${getErrorMessage(error)}`);
		}
	}
}

function bufferToolCall(toolCall, toolCallBuffers, completedToolCallIndices) {
	const index = typeof toolCall.index === "number" ? toolCall.index : 0;
	if (completedToolCallIndices.has(index)) {
		return;
	}
	const buffer = toolCallBuffers.get(index) ?? { args: "" };
	if (typeof toolCall.id === "string") {
		buffer.id = toolCall.id;
	}
	const fn = toolCall.function;
	if (fn && typeof fn.name === "string") {
		buffer.name = fn.name;
	}
	if (fn && typeof fn.arguments === "string") {
		buffer.args += fn.arguments;
	}
	toolCallBuffers.set(index, buffer);
}

function tryEmitBufferedToolCall(toolCall, progress, toolCallBuffers, completedToolCallIndices) {
	const index = typeof toolCall.index === "number" ? toolCall.index : 0;
	const buffer = toolCallBuffers.get(index);
	if (!buffer || !buffer.name) {
		return;
	}
	const parsed = tryParseJSONObject(buffer.args);
	if (!parsed.ok) {
		return;
	}
	const callId = buffer.id || `call_${Math.random().toString(36).slice(2, 10)}`;
	progress.report(new vscode.LanguageModelToolCallPart(callId, buffer.name, parsed.value));
	toolCallBuffers.delete(index);
	completedToolCallIndices.add(index);
}

function flushToolCallBuffers(progress, toolCallBuffers, completedToolCallIndices, throwOnInvalid) {
	for (const [index, buffer] of Array.from(toolCallBuffers.entries())) {
		const argsText = (buffer.args || "").trim() || "{}";
		const parsed = tryParseJSONObject(argsText);
		if (!parsed.ok) {
			if (throwOnInvalid) {
				throw new Error(`Invalid JSON for tool call '${buffer.name || "unknown_tool"}'`);
			}
			continue;
		}
		const callId = buffer.id || `call_${Math.random().toString(36).slice(2, 10)}`;
		progress.report(new vscode.LanguageModelToolCallPart(callId, buffer.name || "unknown_tool", parsed.value));
		toolCallBuffers.delete(index);
		completedToolCallIndices.add(index);
	}
}

function getErrorMessage(error) {
	return error instanceof Error ? error.message : String(error);
}

module.exports = {
	activate,
	deactivate,
};
