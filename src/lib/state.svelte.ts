// État partagé de l'interface : données du cœur, parcours de « Jouer »,
// connexion Microsoft. Les écrans (lib/views) ne font que le lire et appeler
// ses actions.

import {
	api,
	onError,
	onEvent,
	onLauncherUpdate,
	onStopped,
	type LauncherUpdate,
	type CrashSummary,
	type DeviceCode,
	type NewsItem,
	type Overview,
	type PresetsView,
	type ServerStatus,
	type Settings,
	type Updates
} from './api';
import { isPreview } from './preview';

export type View = 'jouer' | 'qualite' | 'options' | 'journal';

type Milestone = { index: number; count: number; label: string; elapsed_ms: number; expected_ms: number };

class LauncherState {
	view = $state<View>('jouer');
	ov = $state<Overview | null>(null);
	s = $state<Settings | null>(null);
	pv = $state<PresetsView | null>(null);
	presetsError = $state<string | null>(null);
	server = $state<ServerStatus | null>(null);
	updates = $state<Updates | null>(null);
	news = $state<NewsItem[]>([]);
	/** Nouvelle version du launcher, et progression de son installation. */
	launcherUpdate = $state<LauncherUpdate | null>(null);
	updating = $state<{ done: number; total: number } | null>(null);
	updateError = $state<string | null>(null);

	// Parcours de « Jouer »
	running = $state(false);
	stage = $state('');
	progress = $state({ done: 0, total: 0 });
	lastLog = $state('');
	logs = $state<string[]>([]);
	milestone = $state<Milestone | null>(null);
	launchStart = $state(0);
	now = $state(Date.now());
	inGame = $state(false);
	crash = $state<CrashSummary | null>(null);
	error = $state<string | null>(null);

	// Connexion Microsoft : par le navigateur (par défaut), ou par code.
	loginMode = $state<'navigateur' | 'code' | null>(null);
	device = $state<DeviceCode | null>(null);
	loginError = $state<string | null>(null);
	copied = $state(false);

	/** Skin du compte (data URL), pour la tête du joueur. */
	skin = $state<string | null>(null);

	playerName = $derived(this.s?.account?.name ?? this.ov?.offline_name ?? null);
	canPlay = $derived(!!this.playerName && !this.running);

	/** Avancement 0–100 : étape en cours, ou démarrage du jeu estimé. */
	percent = $derived.by(() => {
		if (this.inGame) return 100;
		const m = this.milestone;
		if (m) {
			if (m.expected_ms > 0) return Math.min(99, ((this.now - this.launchStart) / m.expected_ms) * 100);
			return ((m.index + 1) / m.count) * 100;
		}
		return this.progress.total > 0 ? (this.progress.done / this.progress.total) * 100 : 0;
	});

	/** Secondes restantes estimées (démarrage du jeu), ou null. */
	remaining = $derived.by(() => {
		const m = this.milestone;
		if (!m || m.expected_ms <= 0 || this.inGame) return null;
		return Math.ceil(Math.max(0, m.expected_ms - (this.now - this.launchStart)) / 1000);
	});

	log(line: string) {
		this.logs.push(line);
		if (this.logs.length > 500) this.logs.splice(0, this.logs.length - 500);
	}

	async refreshOverview() {
		this.ov = await api.overview();
		this.s = this.ov.settings;
		this.refreshSkin();
	}

	refreshSkin() {
		if (!this.s?.account) {
			this.skin = null;
			return;
		}
		api.skin().then((s) => (this.skin = s)).catch(() => (this.skin = null));
	}

	async refreshPresets() {
		try {
			this.pv = await api.presets();
			this.presetsError = null;
		} catch (e) {
			this.presetsError = String(e);
		}
	}

	async save() {
		if (!this.s) return;
		await api.saveSettings($state.snapshot(this.s));
		await this.refreshPresets();
	}

	async play() {
		if (!this.canPlay) return;
		this.running = true;
		this.inGame = false;
		this.crash = null;
		this.error = null;
		this.milestone = null;
		this.progress = { done: 0, total: 0 };
		this.stage = 'Préparation';
		this.lastLog = '';
		try {
			await api.play();
		} catch (e) {
			this.running = false;
			this.error = String(e);
		}
	}

	/** Télécharge, vérifie, installe, puis le launcher redémarre tout seul. */
	async installLauncherUpdate() {
		this.updateError = null;
		this.updating = { done: 0, total: 0 };
		try {
			await api.launcherUpdateInstall();
		} catch (e) {
			this.updating = null;
			this.updateError = String(e);
		}
	}

	async stop() {
		await api.stop();
	}

	/** Connexion par défaut : la page Microsoft s'ouvre, retour automatique. */
	async login() {
		this.loginError = null;
		this.device = null;
		this.loginMode = 'navigateur';
		try {
			const account = await api.loginBrowser();
			if (this.s) this.s.account = account;
			this.refreshSkin();
			this.loginMode = null;
		} catch (e) {
			if (String(e) === 'annulé') return; // remplacé par une autre tentative
			this.loginMode = null;
			this.loginError = String(e);
		}
	}

	async cancelLogin() {
		await api.loginCancel();
		this.loginMode = null;
		this.device = null;
	}

	/** Solution de secours : un code à saisir sur microsoft.com/link. */
	async loginWithCode() {
		await api.loginCancel();
		this.loginError = null;
		this.copied = false;
		this.loginMode = 'code';
		try {
			this.device = await api.loginStart();
			const code = this.device.user_code;
			navigator.clipboard
				?.writeText(code)
				.then(() => (this.copied = true))
				.catch(() => {});
			await api.openUrl(this.device.verification_uri);
			const account = await api.loginFinish();
			if (this.s) this.s.account = account;
			this.refreshSkin();
			this.device = null;
			this.loginMode = null;
		} catch (e) {
			this.loginError = String(e);
			this.device = null;
			this.loginMode = null;
		}
	}

	async logout() {
		await api.logout();
		if (this.s) this.s.account = null;
		this.skin = null;
	}

	/** Branche les événements du cœur ; rend la fonction qui les débranche. */
	start(): () => void {
		const p = new URLSearchParams(location.search);
		const vue = p.get('vue');
		if (vue === 'jouer' || vue === 'qualite' || vue === 'options' || vue === 'journal') this.view = vue;
		if (isPreview && ['prep', 'lancement', 'jeu'].includes(p.get('etat') ?? '')) this.running = true;

		this.refreshOverview().then(() => this.refreshPresets());
		const refreshServer = () => api.serverStatus().then((s) => (this.server = s));
		refreshServer();
		api.checkUpdates().then((u) => (this.updates = u)).catch(() => {});
		api.news().then((n) => (this.news = n)).catch(() => {});
		api.launcherUpdateCheck().then((u) => (this.launcherUpdate = u)).catch(() => {});
		if (isPreview && p.get('etat') === 'code') this.loginWithCode();
		if (isPreview && p.get('etat') === 'lien') this.login();

		const timers = [setInterval(refreshServer, 30_000), setInterval(() => (this.now = Date.now()), 250)];
		const unlisten = [
			onEvent((e) => {
				switch (e.kind) {
					case 'stage':
						this.stage = e.label;
						this.progress = { done: 0, total: 0 };
						this.log(`— ${e.label}`);
						if (e.id === 'launch') this.launchStart = Date.now() - (isPreview ? 36000 : 0);
						break;
					case 'progress':
						this.progress = { done: e.done, total: e.total };
						break;
					case 'log':
						this.lastLog = e.line;
						this.log(e.line);
						break;
					case 'milestone':
						this.milestone = e;
						this.log(`[${e.index + 1}/${e.count}] ${e.label} (${(e.elapsed_ms / 1000).toFixed(1)} s)`);
						break;
					case 'game_ready':
						this.inGame = true;
						this.log(`Jeu prêt en ${(e.elapsed_ms / 1000).toFixed(1)} s`);
						break;
					case 'game_exited':
						this.running = false;
						this.inGame = false;
						this.milestone = null;
						this.crash = e.crash;
						this.log(`Jeu fermé (code ${e.code ?? '?'})`);
						this.refreshOverview();
						api.checkUpdates().then((u) => (this.updates = u)).catch(() => {});
						break;
				}
			}),
			onError((m) => {
				this.running = false;
				this.milestone = null;
				this.error = m;
				this.log(`ERREUR : ${m}`);
			}),
			onLauncherUpdate((p) => (this.updating = p)),
			onStopped(() => {
				this.running = false;
				this.inGame = false;
				this.milestone = null;
				this.log('Arrêté par le joueur');
			})
		];
		const onKey = (e: KeyboardEvent) => {
			if (e.key === 'Enter' && this.view === 'jouer' && !(e.target instanceof HTMLButtonElement)) this.play();
		};
		window.addEventListener('keydown', onKey);
		return () => {
			timers.forEach(clearInterval);
			unlisten.forEach((u) => u.then((f) => f()));
			window.removeEventListener('keydown', onKey);
		};
	}
}

export const L = new LauncherState();
