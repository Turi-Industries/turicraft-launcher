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

/** Une ligne du journal du launcher : heure, texte, et ce qu'elle signale. */
export type LogLine = { t: number; text: string; kind: 'stage' | 'step' | 'error' | 'warn' | 'info' };

function kindOf(text: string): LogLine['kind'] {
	if (text.startsWith('— ')) return 'stage';
	if (/^\[\d+\/\d+\]|^Jeu prêt/.test(text)) return 'step';
	if (/^ERREUR|ERROR|Exception|FATAL|échec|impossible/i.test(text)) return 'error';
	if (/WARN|pilote graphique|carte intégrée/i.test(text)) return 'warn';
	return 'info';
}

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
	/** Recherche lancée depuis Options : son résultat, affiché sous le bouton. */
	updateCheck = $state<'idle' | 'checking' | 'uptodate' | 'found' | 'error'>('idle');
	/** Fenêtre « Nouvelle version » ouverte au démarrage. */
	updatePrompt = $state(false);
	/** « Plus tard » : téléchargée en arrière-plan, installée à la fermeture. */
	updateLater = $state<'idle' | 'downloading' | 'ready' | 'error'>('idle');
	updating = $state<{ done: number; total: number } | null>(null);
	updateError = $state<string | null>(null);

	// Parcours de « Jouer »
	running = $state(false);
	stage = $state('');
	progress = $state({ done: 0, total: 0 });
	lastLog = $state('');
	logs = $state<LogLine[]>([]);
	milestone = $state<Milestone | null>(null);
	launchStart = $state(0);
	now = $state(Date.now());
	inGame = $state(false);
	crash = $state<CrashSummary | null>(null);
	/** Pilote graphique qui ralentit le jeu, vu au dernier lancement. */
	gpuWarning = $state<{ title: string; renderer: string; advice: string } | null>(null);
	/** « Réparer » en cours (Options) : même file que « Jouer », sans lancer le jeu. */
	repairing = $state(false);
	repairDone = $state(false);
	/** « Tout remettre à zéro » : confirmation demandée, fait (le launcher
	 *  redémarre alors tout seul), erreur. */
	resetAsk = $state(false);
	resetDone = $state(false);
	resetError = $state<string | null>(null);
	/** « Réparer le launcher » : réinstallation en cours. */
	reinstalling = $state(false);
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

	log(text: string) {
		this.logs.push({ t: Date.now(), text, kind: kindOf(text) });
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

	/** Numéro de la dernière demande : une réponse plus ancienne arrivée
	 *  après (clics rapprochés) ne doit pas remplacer la plus récente. */
	private presetsRequest = 0;
	async refreshPresets() {
		const n = ++this.presetsRequest;
		try {
			const pv = await api.presets();
			if (n !== this.presetsRequest) return;
			this.pv = pv;
			// Réglages faits en jeu repris par le cœur : l'écran repart d'eux,
			// sinon son prochain enregistrement les effacerait.
			this.s = pv.settings;
			this.presetsError = null;
		} catch (e) {
			if (n !== this.presetsRequest) return;
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
		this.gpuWarning = null;
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

	/** Recherche à la demande (Options) ; au démarrage, elle se fait en silence. */
	async checkLauncherUpdate() {
		this.updateCheck = 'checking';
		this.updateError = null;
		try {
			// Une réponse trop rapide ressemble à un bouton qui n'a rien fait.
			const [u] = await Promise.all([api.launcherUpdateCheck(), new Promise((r) => setTimeout(r, 600))]);
			this.launcherUpdate = u;
			this.updateCheck = u ? 'found' : 'uptodate';
		} catch (e) {
			this.updateError = String(e);
			this.updateCheck = 'error';
		}
	}

	/** « Plus tard » : on télécharge maintenant, on installe en quittant. */
	async installLauncherUpdateLater() {
		this.updatePrompt = false;
		this.updateError = null;
		this.updateLater = 'downloading';
		try {
			await api.launcherUpdateLater();
			this.updateLater = 'ready';
		} catch (e) {
			this.updateLater = 'error';
			this.updateError = String(e);
		}
	}

	/** Revérifie tout, tout de suite (fichiers relus et comparés), sans lancer le jeu. */
	async repair() {
		if (this.running) return;
		this.running = true;
		this.repairing = true;
		this.repairDone = false;
		this.error = null;
		this.stage = 'Vérification';
		this.progress = { done: 0, total: 0 };
		this.log('— Réparation de l’installation');
		try {
			await api.repair();
		} catch (e) {
			this.running = false;
			this.repairing = false;
			this.error = String(e);
		}
	}

	/** Réglages du launcher et du jeu comme à l'installation, compte gardé. */
	async resetSettings() {
		if (this.running) return;
		this.resetAsk = false;
		this.resetError = null;
		try {
			this.s = await api.resetSettings();
			this.resetDone = true;
			this.log('— Réglages remis à zéro (compte gardé)');
			// Le temps de lire « Redémarrage… », puis il repart de zéro,
			// comme à la première ouverture.
			await new Promise((r) => setTimeout(r, 1200));
			await api.restart();
		} catch (e) {
			this.resetError = String(e);
		}
	}

	/** Réinstalle la dernière version du launcher ; il redémarre tout seul. */
	async reinstallLauncher() {
		this.updateError = null;
		this.reinstalling = true;
		this.updating = { done: 0, total: 0 };
		try {
			await api.launcherReinstall();
		} catch (e) {
			this.reinstalling = false;
			this.updating = null;
			this.updateError = String(e);
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
		if (isPreview && ['prep', 'lancement', 'jeu', 'reparation'].includes(p.get('etat') ?? '')) this.running = true;
		if (isPreview && p.get('etat') === 'reparation') this.repairing = true;
		if (isPreview && p.get('etat') === 'raz') this.resetAsk = true;
		if (isPreview && p.get('etat') === 'raz-fait') this.resetDone = true;
		if (isPreview && p.get('journal') === '1') {
			const t0 = Date.now() - 95_000;
			[
				'— Compte',
				'— Java 21',
				'— Minecraft 1.21.1',
				'— Pack Turi Craft',
				'packwiz : 3 fichiers à mettre à jour',
				'réglages faits en jeu repris : distance, images',
				'— Lancement du jeu',
				'[1/7] Démarrage de Java (1.2 s)',
				'[2/7] Mods trouvés (11.4 s)',
				'[3/7] Fenêtre du jeu (20.3 s)',
				'pilote graphique lent : D3D12 (Qualcomm(R) Adreno(TM) X1-85 GPU)',
				'[4/7] Construction des mods (21.0 s)',
				'ERREUR : Le jeu s’est arrêté pendant le chargement (code 1).'
			].forEach((text, i) => this.logs.push({ t: t0 + i * 7000, text, kind: kindOf(text) }));
		}

		this.refreshOverview().then(() => this.refreshPresets());
		const refreshServer = () => api.serverStatus().then((s) => (this.server = s));
		refreshServer();
		api.checkUpdates().then((u) => (this.updates = u)).catch(() => {});
		api.news().then((n) => (this.news = n)).catch(() => {});
		api.launcherUpdateCheck()
			.then((u) => {
				this.launcherUpdate = u;
				// Aperçu : la fenêtre seulement si demandée (&popup=1), pour ne
				// pas masquer les autres captures qui utilisent &maj=1.
				this.updatePrompt = !!u && (!isPreview || p.get('popup') === '1');
			})
			.catch(() => {});
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
					case 'repaired':
						this.running = false;
						this.repairing = false;
						this.repairDone = true;
						this.log('Installation vérifiée et réparée');
						this.refreshOverview();
						break;
					case 'gpu_warning':
						this.gpuWarning = { title: e.title, renderer: e.renderer, advice: e.advice };
						this.log(`${e.title} : ${e.renderer}`);
						break;
					case 'game_exited':
						this.running = false;
						this.inGame = false;
						this.milestone = null;
						this.crash = e.crash;
						this.log(`Jeu fermé (code ${e.code ?? '?'})`);
						// Ce qui a été réglé en jeu apparaît tout de suite dans Qualité.
						this.refreshOverview().then(() => this.refreshPresets());
						api.checkUpdates().then((u) => (this.updates = u)).catch(() => {});
						break;
				}
			}),
			onError((m) => {
				this.running = false;
				this.repairing = false;
				this.milestone = null;
				this.error = m;
				this.log(`ERREUR : ${m}`);
			}),
			onLauncherUpdate((p) => (this.updating = p)),
			onStopped(() => {
				this.running = false;
				this.repairing = false;
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
