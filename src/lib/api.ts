// Appels au cœur Rust (src-tauri/src/lib.rs) et événements qu'il émet.
// Les types reprennent les structures Rust : les garder alignés.

import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { isPreview, previewError, previewInvoke, previewListen } from './preview';

// Hors de Tauri (navigateur), un faux cœur répond : voir preview.ts.
function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
	return isPreview ? (previewInvoke(cmd) as Promise<T>) : tauriInvoke<T>(cmd, args);
}

export interface Account {
	name: string;
	uuid: string;
}

export type Behavior = 'reduire' | 'garder' | 'fermer';

export interface Settings {
	preset: string; // auto | faible | moyen | haut | personnalise (Avancé)
	custom_base: string | null;
	custom_groups: string[];
	custom_memory_gb: number | null;
	toggles: Record<string, boolean>;
	sliders: Record<string, number>;
	mods: Record<string, boolean>;
	join_server: boolean;
	launcher_behavior: Behavior;
	account: Account | null;
	last_milestones_ms: number[];
}

export interface Hardware {
	ram_gb: number;
	cpu_threads: number;
	gpu_name: string;
	gpu_dedicated: boolean;
	vram_gb: number;
	display: { refresh_hz: number | null; vrr_capable: boolean; vrr_active: boolean; source: string };
}

export interface Overview {
	settings: Settings;
	hardware: Hardware;
	pack_version: string | null;
	offline_name: string | null;
	data_dir: string;
	disk_free_gb: number | null;
	launcher_version: string;
}

export interface Preset {
	label: string;
	description: string;
	groups: string[];
	toggles: Record<string, boolean>;
}

export interface Slider {
	label: string;
	description: string;
	min: number;
	max: number;
	step: number;
	unit: string;
	requires: string | null;
}

export interface PresetsView {
	/** Choix du joueur effacés quand il choisit un préréglage. */
	preset_owned: { toggles: string[]; sliders: string[]; mods: string[] };
	settings: Settings;
	file: {
		optional_groups: Record<string, string[]>;
		toggles: Record<
			string,
			{ label: string; description: string; category: string; default: boolean; groups: string[] }
		>;
		group_info: Record<string, { label: string; description: string }>;
		sliders: Record<string, Slider>;
		presets: Record<string, Preset>;
	};
	detected: string;
	resolved: {
		preset: string;
		groups: string[];
		toggles: Record<string, boolean>;
		sliders: Record<string, number>;
		memory_gb: number;
		gc: string;
		adapted: Record<string, string>;
	};
	memory_cap_gb: number;
	/** Mémoire conseillée pour cette machine. */
	memory_auto_gb: number;
}

/** « 5,5 Go » : les demi-Go s'écrivent à la française. */
export function go(n: number): string {
	return `${n.toLocaleString('fr-FR', { maximumFractionDigits: 1 })} Go`;
}

export interface ServerStatus {
	online: boolean;
	players: number;
	max_players: number;
	version: string;
	motd: string;
	latency_ms: number;
	error: string | null;
}

export interface Updates {
	pack_installed: string | null;
	pack_online: string | null;
	launcher_current: string;
}

export interface LauncherUpdate {
	version: string;
	notes: string;
}

export interface NewsItem {
	date: string;
	title: string;
	body: string;
	url?: string;
}

export interface DeviceCode {
	user_code: string;
	verification_uri: string;
	expires_in: number;
}

export interface CrashSummary {
	description: string;
	cause: string;
	report: string | null;
	first_error: string | null;
	cascade: boolean;
	native: boolean;
}

export type LauncherEvent =
	| { kind: 'stage'; id: string; label: string }
	| { kind: 'progress'; done: number; total: number }
	| { kind: 'log'; line: string }
	| {
			kind: 'milestone';
			index: number;
			count: number;
			label: string;
			elapsed_ms: number;
			expected_ms: number;
	  }
	| { kind: 'game_ready'; elapsed_ms: number }
	| { kind: 'gpu_warning'; title: string; renderer: string; advice: string }
	| { kind: 'repaired' }
	| { kind: 'game_exited'; code: number | null; crash: CrashSummary | null };

export const api = {
	overview: () => invoke<Overview>('overview'),
	saveSettings: (settings: Settings) => invoke<void>('save_settings', { settings }),
	presets: () => invoke<PresetsView>('presets_view'),
	serverStatus: () => invoke<ServerStatus>('server_status'),
	checkUpdates: () => invoke<Updates>('check_updates'),
	launcherUpdateCheck: () => invoke<LauncherUpdate | null>('launcher_update_check'),
	launcherUpdateInstall: () => invoke<void>('launcher_update_install'),
	launcherReinstall: () => invoke<void>('launcher_reinstall'),
	launcherUpdateLater: () => invoke<void>('launcher_update_later'),
	news: () => invoke<NewsItem[]>('news'),
	loginBrowser: () => invoke<Account>('login_browser'),
	loginCancel: () => invoke<void>('login_cancel'),
	loginStart: () => invoke<DeviceCode>('login_start'),
	loginFinish: () => invoke<Account>('login_finish'),
	logout: () => invoke<void>('logout'),
	skin: () => invoke<string>('skin'),
	repair: () => invoke<void>('repair'),
	openFolder: (which: 'instance' | 'logs' | 'crash' | 'screenshots') =>
		invoke<void>('open_folder', { which }),
	openUrl: (url: string) => invoke<void>('open_url', { url }),
	play: () => invoke<void>('play'),
	stop: () => invoke<void>('stop')
};

export function onEvent(cb: (e: LauncherEvent) => void): Promise<UnlistenFn> {
	if (isPreview) return previewListen(cb);
	return listen<LauncherEvent>('launcher', (e) => cb(e.payload));
}

export function onError(cb: (message: string) => void): Promise<UnlistenFn> {
	if (isPreview) return previewError(cb);
	return listen<string>('launcher-error', (e) => cb(e.payload));
}

export function onLauncherUpdate(cb: (p: { done: number; total: number }) => void): Promise<UnlistenFn> {
	if (isPreview) return Promise.resolve(() => {});
	return listen<{ done: number; total: number }>('launcher-update', (e) => cb(e.payload));
}

export function onStopped(cb: () => void): Promise<UnlistenFn> {
	if (isPreview) return Promise.resolve(() => {});
	return listen('launcher-stopped', () => cb());
}
