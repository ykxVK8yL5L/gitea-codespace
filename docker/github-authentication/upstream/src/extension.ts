/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/

import * as vscode from 'vscode';
import { GitHubAuthenticationProvider, UriEventHandler } from './github';

const settingNotSent = '"github-enterprise.uri" not set';
const settingInvalid = '"github-enterprise.uri" invalid';

class NullAuthProvider implements vscode.AuthenticationProvider {
	private _onDidChangeSessions = new vscode.EventEmitter<vscode.AuthenticationProviderAuthenticationSessionsChangeEvent>();
	onDidChangeSessions = this._onDidChangeSessions.event;

	private readonly _disposable: vscode.Disposable;

	constructor(private readonly _errorMessage: string) {
		this._disposable = vscode.authentication.registerAuthenticationProvider('github-enterprise', 'GitHub Enterprise', this);
	}

	createSession(): Thenable<vscode.AuthenticationSession> {
		throw new Error(this._errorMessage);
	}

	getSessions(): Thenable<vscode.AuthenticationSession[]> {
		return Promise.resolve([]);
	}
	removeSession(): Thenable<void> {
		throw new Error(this._errorMessage);
	}

	dispose() {
		this._onDidChangeSessions.dispose();
		this._disposable.dispose();
	}
}

function initGHES(context: vscode.ExtensionContext, uriHandler: UriEventHandler): vscode.Disposable {
	const settingValue = vscode.workspace.getConfiguration().get<string>('github-enterprise.uri');
	if (!settingValue) {
		const provider = new NullAuthProvider(settingNotSent);
		context.subscriptions.push(provider);
		return provider;
	}

	// validate user value
	let uri: vscode.Uri;
	try {
		uri = vscode.Uri.parse(settingValue, true);
	} catch (e) {
		vscode.window.showErrorMessage(vscode.l10n.t('GitHub Enterprise Server URI is not a valid URI: {0}', e.message ?? e));
		const provider = new NullAuthProvider(settingInvalid);
		context.subscriptions.push(provider);
		return provider;
	}

	const githubEnterpriseAuthProvider = new GitHubAuthenticationProvider(context, uriHandler, uri);
	context.subscriptions.push(githubEnterpriseAuthProvider);
	return githubEnterpriseAuthProvider;
}

export function activate(context: vscode.ExtensionContext) {
	const uriHandler = new UriEventHandler();
	context.subscriptions.push(uriHandler);
	context.subscriptions.push(vscode.window.registerUriHandler(uriHandler));

	const githubAuthProvider = new GitHubAuthenticationProvider(context, uriHandler);
	context.subscriptions.push(githubAuthProvider);
	if (process.env.GITHUB_AUTHENTICATION_MODE?.trim() === 'env-token' && (process.env.GITHUB_TOKEN?.trim() || process.env.WORKSPACE_GITHUB_TOKEN?.trim())) {
		void githubAuthProvider.createSession(['user:email']).then(session => {
			if (session) {
				vscode.window.showInformationMessage(vscode.l10n.t('Signed in to GitHub from environment token as {0}.', session.account.label));
				setTimeout(() => {
					void vscode.commands.executeCommand('github.copilot.refreshToken').then(undefined, err => {
						console.warn('Failed to refresh Copilot status', err);
					});
				}, 3000);
			}
		}, e => {
			vscode.window.showWarningMessage(vscode.l10n.t('GitHub environment token sign in failed: {0}', `${e}`));
		});
	} else if (process.env.GITHUB_AUTHENTICATION_MODE?.trim() === 'env-token') {
		vscode.window.showWarningMessage(vscode.l10n.t('GitHub env-token mode is enabled, but no token was found.'));
	}

	context.subscriptions.push(vscode.commands.registerCommand('github-authentication.signIn', async () => {
		try {
			const session = await vscode.authentication.getSession(
				'github',
				['user:email'],
				{ createIfNone: true }
			);

			if (session) {
				vscode.window.showInformationMessage(vscode.l10n.t('Signed in to GitHub as {0}.', session.account.label));
			}
		} catch (e) {
			vscode.window.showErrorMessage(vscode.l10n.t('GitHub sign in failed: {0}', `${e}`));
			throw e;
		}
	}));
	context.subscriptions.push(vscode.commands.registerCommand('github-authentication.signInWithPAT', async () => {
		const token = await vscode.window.showInputBox({
			title: vscode.l10n.t('GitHub: Sign In with PAT'),
			prompt: vscode.l10n.t('Enter a GitHub Personal Access Token.'),
			password: true,
			ignoreFocusOut: true,
			validateInput: value => value.trim() ? undefined : vscode.l10n.t('A Personal Access Token is required.')
		});

		if (!token) {
			return;
		}

		try {
			const session = await githubAuthProvider.createSessionWithPAT(token.trim(), ['user:email']);
			vscode.window.showInformationMessage(vscode.l10n.t('Signed in to GitHub with PAT as {0}.', session.account.label));
		} catch (e) {
			vscode.window.showErrorMessage(vscode.l10n.t('GitHub PAT sign in failed: {0}', `${e}`));
			throw e;
		}
	}));

	let before = vscode.workspace.getConfiguration().get<string>('github-enterprise.uri');
	let githubEnterpriseAuthProvider = initGHES(context, uriHandler);
	context.subscriptions.push(vscode.workspace.onDidChangeConfiguration(e => {
		if (e.affectsConfiguration('github-enterprise.uri')) {
			const after = vscode.workspace.getConfiguration().get<string>('github-enterprise.uri');
			if (before !== after) {
				githubEnterpriseAuthProvider?.dispose();
				before = after;
				githubEnterpriseAuthProvider = initGHES(context, uriHandler);
			}
		}
	}));

	// Listener to prompt for reload when the fetch implementation setting changes
	const beforeFetchSetting = vscode.workspace.getConfiguration().get<boolean>('github-authentication.useElectronFetch', true);
	context.subscriptions.push(vscode.workspace.onDidChangeConfiguration(async e => {
		if (e.affectsConfiguration('github-authentication.useElectronFetch')) {
			const afterFetchSetting = vscode.workspace.getConfiguration().get<boolean>('github-authentication.useElectronFetch', true);
			if (beforeFetchSetting !== afterFetchSetting) {
				const selection = await vscode.window.showInformationMessage(
					vscode.l10n.t('GitHub Authentication - Reload required'),
					{
						modal: true,
						detail: vscode.l10n.t('A reload is required for the fetch setting change to take effect.')
					},
					vscode.l10n.t('Reload Window')
				);
				if (selection) {
					await vscode.commands.executeCommand('workbench.action.reloadWindow');
				}
			}
		}
	}));
}
