// Mode aperçu : l'interface hors de Tauri (navigateur, `bun run dev`), avec un
// faux cœur. Sert à regarder chaque écran et à en faire des captures
// (scripts/captures.sh). Jamais dans une version
// publiée : n'est chargé que si Tauri est absent.
//
// L'état se choisit dans l'URL : ?vue=jouer|qualite|options|journal
// &etat=repos|prep|lancement|jeu|crash|pilote|integree|reparation|repare|raz|raz-fait|code|hors-ligne|pack-hs &compte=0 &serveur=0 &maj=1
// &preset=auto|faible|moyen|haut|personnalise &perso=1 (une option changée)
// &classement=0 (classement du Snake injoignable) &journal=1 (journal rempli) &dossiers=1 (menu Dossiers ouvert) &angle=35 (« Jouer » figé sous cet angle)

import type { LauncherEvent } from './api';

export const isPreview = typeof window !== 'undefined' && !('__TAURI_INTERNALS__' in window);

const params = () => new URLSearchParams(typeof location !== 'undefined' ? location.search : '');

const presetsFile = {
	optional_groups: {
		animations: [],
		joueur: [],
		premiere_personne: [],
		lumieres: [],
		particules: [],
		objets_physiques: [],
		textures_connectees: [],
		interface_animee: [],
		aeronautics_visuel: [],
		sons_legers: [],
		sons_ambiance: []
	},
	toggles: {
		shaders: { category: 'graphismes', label: 'Shaders', description: 'Ombres, reflets, eau réaliste. Demande une bonne carte graphique.', default: true, groups: [] },
		vue_lointaine: { category: 'graphismes', label: 'Vue lointaine', description: 'Distant Horizons : le paysage reste visible bien au-delà de la distance d’affichage.', default: true, groups: [] },
		son_3d: { category: 'mods', label: 'Son 3D réaliste', description: 'Écho dans les grottes, sons étouffés derrière les murs.', default: true, groups: [] },
		objets_physiques: { category: 'mods', label: 'Objets au sol réalistes', description: 'Les objets tombent à plat au lieu de flotter en tournant.', default: true, groups: ['objets_physiques'] },
		premiere_personne: { category: 'mods', label: 'Corps en première personne', description: 'Voir ses jambes et son corps en regardant vers le bas.', default: false, groups: ['premiere_personne'] },
		synchro_verticale: { category: 'graphismes', label: 'Synchronisation verticale', description: 'Évite les déchirures de l’image. La couper peut donner plus d’images par seconde.', default: true, groups: [] },
		plein_ecran: { category: 'graphismes', label: 'Plein écran', description: 'Le jeu charge dans une fenêtre, puis passe en plein écran (F11 en jeu pour basculer).', default: true, groups: [] },
		infobulle: { category: 'interface', label: 'Infobulle de ce que tu regardes', description: 'Jade : nom du bloc ou de la créature visée, en haut de l’écran.', default: true, groups: [] },
		titres_biomes: { category: 'interface', label: 'Titres des biomes', description: 'Le nom du biome s’affiche quand tu y entres.', default: true, groups: [] },
		balancement: { category: 'graphismes', label: 'Balancement de la vue', description: 'La caméra bouge en marchant. À couper si ça donne mal au cœur.', default: true, groups: [] }
	},
	group_info: {
		animations: { label: 'Animations des créatures', description: 'Fresh Animations : les mobs clignent des yeux, respirent, bougent la tête.' },
		joueur: { label: 'Skins 3D, capes, animations', description: 'Couches de skin en relief, capes qui flottent, bras qui suivent les mouvements.' },
		lumieres: { label: 'Lumières dynamiques', description: 'Une torche en main éclaire autour de toi.' },
		particules: { label: 'Particules supplémentaires', description: 'Feuilles qui tombent, bulles qui éclatent, lucioles.' },
		textures_connectees: { label: 'Textures connectées', description: 'Le verre et les bibliothèques se raccordent entre blocs voisins.' },
		interface_animee: { label: 'Chat animé', description: 'Les messages du chat glissent à l’écran.' },
		aeronautics_visuel: { label: 'Flou des hélices', description: 'Les hélices d’Aeronautics se floutent en tournant vite.' },
		sons_legers: { label: 'Bruits de pas et de gouttes', description: 'Pas selon le sol, gouttes qui tombent, sons des engins Create.' },
		sons_ambiance: { label: 'Ambiance sonore complète', description: 'AmbientSounds : vent, oiseaux, grottes, selon le biome. Plus gourmand.' }
	},
	sliders: {
		distance: { label: 'Distance d’affichage', description: 'Chunks chargés autour de toi. Le plus gros levier de performance.', min: 2, max: 32, step: 1, unit: 'chunks', requires: null },
		distance_lointaine: { label: 'Distance de la vue lointaine', description: 'Jusqu’où Distant Horizons dessine le paysage.', min: 32, max: 256, step: 16, unit: 'chunks', requires: 'vue_lointaine' },
		interface: { label: 'Taille de l’interface', description: 'Menus, inventaire, barre de vie. Réglée sur la définition de ton écran.', min: 1, max: 6, step: 1, unit: 'gui', requires: null },
		images: { label: 'Images par seconde maximum', description: 'Réglé sur la fréquence de ton écran : au-delà, les images ne s’affichent pas. 260 = illimité.', min: 30, max: 260, step: 10, unit: 'fps', requires: null }
	},
	choices: {
		shader_pack: {
			category: 'graphismes',
			label: 'Pack de shaders',
			description: 'Quel pack de shaders utiliser.',
			default: 'unbound',
			requires: 'shaders',
			values: [
				{ id: 'unbound', label: 'Unbound', description: 'Complementary Unbound : lumière, ciel et eau plus travaillés.' },
				{ id: 'reimagined', label: 'Reimagined', description: 'Complementary Reimagined : plus proche de Minecraft d’origine.' },
				{ id: 'autre', label: 'Autre', description: 'Un pack ajouté dans le dossier shaderpacks et choisi en jeu (Options vidéo, Shaders) : le launcher n’y touche pas.', other: true }
			]
		}
	},
	presets: {
		faible: { label: 'Faible', description: 'Petites machines, 8 Go de RAM : l’essentiel.', groups: [], toggles: { shaders: false, vue_lointaine: false, son_3d: false, objets_physiques: false } },
		moyen: { label: 'Moyen', description: 'Animations et lumières, vue à 1 km.', groups: ['animations', 'joueur', 'lumieres'], toggles: { shaders: false, objets_physiques: false } },
		haut: { label: 'Haut', description: 'Tout activé, vue à 2 km.', groups: ['animations', 'joueur', 'lumieres', 'particules'], toggles: {} }
	}
};

function settings() {
	const p = params();
	return {
		preset: p.get('preset') ?? 'auto',
		custom_base: p.get('preset') === 'personnalise' ? 'haut' : null,
		custom_groups: p.get('preset') === 'personnalise' ? ['animations', 'joueur', 'lumieres'] : [],
		custom_memory_gb: p.get('preset') === 'personnalise' ? 8.5 : null,
		toggles: p.get('perso') === '1' ? { vue_lointaine: false } : {},
		sliders: p.get('perso') === '1' ? { distance: 10 } : {},
		mods: p.get('perso') === '1' ? { sons_ambiance: false } : {},
		choices: p.get('perso') === '1' ? { shader_pack: 'autre' } : {},
		join_server: false,
		launcher_behavior: 'reduire',
		account: p.get('compte') === '0' ? null : { name: 'Joueur_Turi', uuid: '00000000000040008000000000000000' },
		last_milestones_ms: [1000, 11000, 20000, 21000, 36000, 60000, 78000]
	};
}

type Handler = (e: LauncherEvent) => void;
const handlers: Handler[] = [];
export function previewListen(cb: Handler) {
	handlers.push(cb);
	// États figés demandés par l'URL
	const etat = params().get('etat');
	setTimeout(() => {
		const emit = (e: LauncherEvent) => handlers.forEach((h) => h(e));
		if (etat === 'prep') {
			emit({ kind: 'stage', id: 'neoforge', label: 'NeoForge 21.1.251' });
			emit({ kind: 'progress', done: 312, total: 747 });
			// Ligne réelle de l'installeur NeoForge : un chemin sans espace, très long.
			emit({ kind: 'log', line: 'Extracting: /home/joueur/.local/share/turicraft/minecraft/libraries/net/neoforged/neoforge/21.1.251/neoforge-21.1.251-universal.jar/data/neoforge/loot_modifiers/global_loot_modifiers.json' });
		} else if (etat === 'lancement' || etat === 'jeu') {
			emit({ kind: 'stage', id: 'launch', label: 'Lancement du jeu' });
			emit({ kind: 'milestone', index: 4, count: 7, label: 'Chargement des ressources', elapsed_ms: 36000, expected_ms: 78000 });
			if (etat === 'jeu') emit({ kind: 'game_ready', elapsed_ms: 78000 });
		} else if (etat === 'reparation') {
			emit({ kind: 'stage', id: 'pack', label: 'Pack Turi Craft' });
			emit({ kind: 'progress', done: 214, total: 431 });
		} else if (etat === 'repare') {
			emit({ kind: 'repaired' });
		} else if (etat === 'integree') {
			// Texte de launch.rs (diag::integrated_instead_of_dedicated).
			emit({
				kind: 'gpu_warning',
				title: 'Le jeu tourne sur la carte graphique intégrée',
				renderer: 'Intel(R) UHD Graphics',
				advice:
					'Ta machine a une carte plus puissante (NVIDIA GeForce RTX 4070 Laptop GPU). Dans Windows : Paramètres → Système → Affichage → Graphiques → javaw.exe (dans le dossier turicraft) → Hautes performances. Puis relance le jeu.'
			});
		} else if (etat === 'mods') {
			// packwiz::set_aside_unknown_mods
			emit({ kind: 'mods_set_aside', files: ['wurst-neoforge.jar', 'xray-ultimate.jar'] });
		} else if (etat === 'pilote') {
			// Texte de diag::slow_gl_driver (Snapdragon sans pilote OpenGL natif).
			emit({
				kind: 'gpu_warning',
				title: 'Pilote graphique à mettre à jour',
				renderer: 'D3D12 (Qualcomm(R) Adreno(TM) X1-85 GPU)',
				advice:
					'OpenGL passe par une couche de compatibilité DirectX 12 : le jeu tourne bien plus lentement. Installe le dernier pilote graphique de ton PC (Windows Update → Options avancées → Mises à jour facultatives, ou le site du fabricant).'
			});
		} else if (etat === 'crash') {
			emit({
				kind: 'game_exited',
				code: 1,
				crash: {
					description: 'Exception while adding particle',
					cause: 'java.lang.NullPointerException: Cannot invoke "java.util.List.size()" because "this.sprites" is null',
					report: '/home/x/crash-reports/crash-2026-09-24_16.21.37-client.txt',
					first_error: '[16:21:37] [Render thread/ERROR] SplashParticle.setSpriteFromAge',
					cascade: false,
					native: false
				}
			});
		}
	}, 50);
	return Promise.resolve(() => {});
}

/** Erreurs de préparation (lib.rs, play_inner), mêmes textes. */
export function previewError(cb: (message: string) => void) {
	const etat = params().get('etat');
	const messages: Record<string, string> = {
		'hors-ligne':
			'Pas de connexion Internet.\nElle est nécessaire pour vérifier ton compte et les mises à jour du pack avant de jouer. Vérifie ta connexion, puis relance.',
		'pack-hs':
			"Le serveur du pack (pack.turi-industries.eu) ne répond pas, alors qu'Internet fonctionne.\nRéessaie dans quelques minutes ; si ça dure, préviens un admin."
	};
	if (etat && messages[etat]) setTimeout(() => cb(messages[etat]), 50);
	return Promise.resolve(() => {});
}

export async function previewInvoke(cmd: string): Promise<unknown> {
	const p = params();
	switch (cmd) {
		case 'overview':
			return {
				settings: settings(),
				hardware: {
					ram_gb: 23.2,
					cpu_threads: 20,
					gpu_name: 'AMD Radeon RX 7900 XT',
					gpu_dedicated: true,
					vram_gb: 20,
					display: { refresh_hz: 144, vrr_capable: true, vrr_active: true, source: 'KDE, écran DP-2' }
				},
				pack_version: '0.1.0',
				offline_name: null,
				data_dir: '/home/joueur/.local/share/turicraft',
				launcher_version: '0.1.0',
				disk_free_gb: 19
			};
		case 'presets_view':
			return {
				file: presetsFile,
				detected: 'haut',
				resolved: {
					preset: 'haut',
					groups: ['animations', 'joueur', 'lumieres', 'particules', 'textures_connectees', 'aeronautics_visuel', 'sons_legers', ...(p.get('perso') === '1' ? [] : ['sons_ambiance'])],
					toggles: { shaders: true, vue_lointaine: p.get('perso') !== '1', son_3d: true, objets_physiques: true, premiere_personne: true, synchro_verticale: false, plein_ecran: true, infobulle: true, titres_biomes: true, balancement: true },
					sliders: { distance: 16, distance_lointaine: 192, interface: 3, images: 141 },
					choices: { shader_pack: p.get('perso') === '1' ? 'autre' : 'unbound' },
					adapted: p.get('perso') === '1' ? { distance_lointaine: 'grosse carte graphique' } : { distance: 'grosse carte graphique', distance_lointaine: 'grosse carte graphique', images: 'écran FreeSync / G-Sync actif', interface: 'définition de ton écran principal', synchro_verticale: 'écran FreeSync / G-Sync actif' },
					memory_gb: 10,
					gc: 'ZGC'
				},
				memory_cap_gb: 30,
				memory_auto_gb: 10,
				settings: settings(),
				preset_owned: {
					toggles: ['shaders', 'vue_lointaine', 'son_3d', 'objets_physiques', 'premiere_personne', 'synchro_verticale'],
					sliders: ['distance', 'distance_lointaine'],
					mods: ['animations', 'joueur', 'lumieres', 'particules', 'textures_connectees', 'interface_animee', 'aeronautics_visuel', 'sons_legers', 'sons_ambiance']
				}
			};
		case 'server_status':
			return p.get('serveur') === '0'
				? { online: false, players: 0, max_players: 0, version: '', latency_ms: 0, motd: '', error: 'pas de réponse en 5 s' }
				: { online: true, players: 3, max_players: 20, version: '1.21.1', latency_ms: 42, motd: 'Turi Craft V2', error: null };
		case 'check_updates':
			return { pack_installed: '0.1.0', pack_online: p.get('maj') ? '0.2.0' : '0.1.0', launcher_current: '0.1.0' };
		case 'launcher_update_later':
			await new Promise((r) => setTimeout(r, 1500));
			return null;
		case 'launcher_update_check':
			return p.get('maj') ? { version: '0.2.0', notes: 'Connexion par lien, réglages selon l’écran.' } : null;
		case 'skin': {
			// Aperçu : un skin posé temporairement dans static/ (jamais commité).
			const r = await fetch('/skin-apercu.png');
			if (!r.ok) throw new Error('pas de skin');
			const b = await r.blob();
			return await new Promise((ok) => {
				const fr = new FileReader();
				fr.onload = () => ok(fr.result);
				fr.readAsDataURL(b);
			});
		}
		case 'reset_settings':
			return { ...settings(), preset: 'auto', toggles: {}, sliders: {}, mods: {}, choices: {} };
		case 'news':
			return [
				{ date: '2026-09-24', title: 'Le launcher Turi Craft arrive', body: 'Installation du jeu, des mods et des réglages en un clic, préréglages de qualité adaptés à ta machine, et connexion avec ton compte Microsoft.' },
				{ date: '2026-09-24', title: 'Nouveaux menus', body: 'Menu principal et menu pause refaits.\nDans l’esprit de Minecraft.', url: 'https://discord.com/channels/1/2/3' }
			];
		case 'snake_top':
		case 'snake_submit': {
			const top = [
				['Turi_Boss', 87],
				['Pikachu_Fan', 64],
				['CreateMaster', 51],
				['Joueur_Turi', 38],
				['Aeronaute', 30],
				['Eevee42', 22],
				['Steve', 17],
				['Alex', 9]
			].map(([name, score], i) => ({ name, uuid: `u${i}`, score, date: '2026-09-26' }));
			if (p.get('classement') === '0') throw new Error('classement injoignable');
			return cmd === 'snake_top' ? top : { top, rank: 4, best: 38 };
		}
		case 'login_start':
			return { user_code: 'Y8SWJLX3', verification_uri: 'https://www.microsoft.com/link', expires_in: 900 };
		case 'login_browser':
			return new Promise(() => {}); // le joueur n'a pas encore choisi son compte
		case 'login_finish':
			return new Promise(() => {}); // attend indéfiniment, comme un joueur qui n'a pas encore saisi le code
		default:
			return null;
	}
}
