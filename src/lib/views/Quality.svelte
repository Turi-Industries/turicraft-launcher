<script lang="ts">
	import { go } from '$lib/api';
	import { L } from '$lib/state.svelte';

	// Ordre d'affichage ; « auto » n'est pas un préréglage du fichier.
	const LEVELS = ['faible', 'moyen', 'haut'];
	const SECTIONS = [
		{ id: 'graphismes', title: 'Graphismes' },
		{ id: 'mods', title: 'Mods' },
		{ id: 'interface', title: 'Interface' }
	];

	const advanced = $derived(L.s?.preset === 'personnalise');
	const presets = $derived(L.pv?.file.presets ?? {});
	const detectedLabel = $derived(L.pv ? presets[L.pv.detected]?.label : '');
	const toggles = $derived(Object.entries(L.pv?.file.toggles ?? {}));
	// Mods optionnels qu'aucune option ne pilote déjà : un interrupteur chacun.
	const modGroups = $derived.by(() => {
		if (!L.pv) return [];
		const byToggle = new Set(toggles.flatMap(([, t]) => t.groups ?? []));
		return Object.keys(L.pv.file.group_info ?? {}).filter((g) => !byToggle.has(g) && g in L.pv!.file.optional_groups);
	});
	const changed = $derived(
		!!L.s &&
			(Object.keys(L.s.toggles).length > 0 || Object.keys(L.s.sliders).length > 0 || Object.keys(L.s.mods ?? {}).length > 0)
	);

	/** Valeur affichée d'un curseur : celle du joueur, sinon la conseillée. */
	const sliderValue = (id: string) => L.s?.sliders[id] ?? L.pv?.resolved.sliders[id] ?? 0;
	const km = (chunks: number) => ((chunks * 16) / 1000).toLocaleString('fr-FR', { maximumFractionDigits: 1 });
	/** Images/s réglées sur l'écran (pas changées par le joueur) : on affiche
	 *  la fréquence de l'écran, pas la limite FreeSync (fréquence − 3). */
	const followsScreen = (id: string) =>
		id === 'images' && L.s?.sliders[id] === undefined && !!L.pv?.resolved.adapted[id] && !!L.ov?.hardware.display?.refresh_hz;
	function sliderText(id: string, unit: string, max: number) {
		const v = sliderValue(id);
		if (followsScreen(id)) return `Écran : ${L.ov!.hardware.display.refresh_hz} Hz`;
		if (unit === 'fps') return v >= max ? 'Illimité' : `${v} images/s`;
		if (unit === 'gui') return `×${v}`;
		if (id === 'distance') return `${v} chunks · ${v * 16} blocs`;
		return `${v} chunks · ${km(v)} km`;
	}
	const totalView = $derived.by(() => {
		if (!L.pv) return '';
		const far = L.pv.resolved.toggles['vue_lointaine'] ? sliderValue('distance_lointaine') : 0;
		return `${km(Math.max(sliderValue('distance'), far))} km`;
	});

	/** Choisir un préréglage, c'est lui confier ce qu'il décide (distances,
	 *  shaders, mods…) : les choix du joueur sur ceux-là s'effacent. Ses
	 *  préférences (balancement, taille de l'interface…) restent. */
	function pick(id: string) {
		if (!L.s || L.running) return;
		const owned = L.pv?.preset_owned;
		if (owned) {
			for (const t of owned.toggles) delete L.s.toggles[t];
			for (const sl of owned.sliders) delete L.s.sliders[sl];
			const mods = { ...(L.s.mods ?? {}) };
			for (const g of owned.mods) delete mods[g];
			L.s.mods = mods;
		}
		L.s.preset = id;
		L.save();
	}

	/** Passer en Avancé part du préréglage en cours, pour ne rien perdre. */
	function setAdvanced(on: boolean) {
		if (!L.s || !L.pv || L.running) return;
		if (on) {
			L.s.custom_base = L.pv.resolved.preset;
			L.s.custom_memory_gb = L.pv.resolved.memory_gb;
			L.s.preset = 'personnalise';
		} else {
			L.s.preset = 'auto';
		}
		L.save();
	}

	function setBase(id: string) {
		if (!L.s || !L.pv) return;
		L.s.custom_base = id;
		L.s.custom_memory_gb = L.pv.memory_auto_gb;
		L.save();
	}

	function setToggle(id: string, on: boolean) {
		if (!L.s) return;
		L.s.toggles[id] = on;
		L.save();
	}
	function setMod(g: string, on: boolean) {
		if (!L.s) return;
		L.s.mods = { ...(L.s.mods ?? {}), [g]: on };
		L.save();
	}
	function setSlider(id: string, v: number, save: boolean) {
		if (!L.s) return;
		L.s.sliders[id] = v;
		if (save) L.save();
	}
	function resetAll() {
		if (!L.s) return;
		L.s.toggles = {};
		L.s.sliders = {};
		L.s.mods = {};
		L.save();
	}
</script>

{#snippet tags(id: string, mine: boolean)}
	{#if mine}<span class="tag mine">modifié</span>
	{:else if L.pv?.resolved.adapted[id]}<span class="tag auto" title={`Ajusté pour ta machine : ${L.pv.resolved.adapted[id]}`}>auto</span>{/if}
{/snippet}

<div class="page">
	<header>
		<h2>Qualité</h2>
		<div class="segmented" role="tablist">
			<button class:on={!advanced} onclick={() => setAdvanced(false)} disabled={L.running}>Simple</button>
			<button class:on={advanced} onclick={() => setAdvanced(true)} disabled={L.running}>Avancé</button>
		</div>
	</header>

	{#if L.ov}
		<p class="hint machine">
			Ta machine : {L.ov.hardware.ram_gb.toFixed(0)} Go de RAM · {L.ov.hardware.cpu_threads} cœurs ·
			{L.ov.hardware.gpu_name}{L.ov.hardware.gpu_dedicated ? ` (${L.ov.hardware.vram_gb.toFixed(0)} Go)` : ' (intégrée)'}
			{#if L.ov.hardware.display?.refresh_hz}
				<span class="screen" title={L.ov.hardware.display.source}
					>Écran principal : {L.ov.hardware.display.refresh_hz} Hz{L.ov.hardware.display.vrr_active
						? ', FreeSync / G-Sync actif'
						: L.ov.hardware.display.vrr_capable
							? ', compatible FreeSync / G-Sync'
							: ''}</span
				>
			{/if}
		</p>
	{/if}

	{#if L.presetsError}
		<p class="panel problem">Préréglages indisponibles : {L.presetsError}</p>
	{:else if !L.pv || !L.s}
		<p class="hint">Chargement…</p>
	{:else}
		{#if !advanced}
			<div class="cards">
				<button class="card" class:on={L.s.preset === 'auto'} onclick={() => pick('auto')} disabled={L.running}>
					<span class="title">Automatique</span>
					<span class="desc">Choisi pour ta machine : <strong>{detectedLabel}</strong>.</span>
				</button>
				{#each LEVELS as id (id)}
					{#if presets[id]}
						<button class="card" class:on={L.s.preset === id} onclick={() => pick(id)} disabled={L.running}>
							<span class="title">
								{presets[id].label}
								{#if id === L.pv.detected}<span class="badge">Conseillé</span>{/if}
							</span>
							<span class="desc">{presets[id].description}</span>
						</button>
					{/if}
				{/each}
			</div>
		{:else}
			<div class="panel">
				<div class="section-title">Point de départ</div>
				<div class="segmented">
					{#each LEVELS as id (id)}
						{#if presets[id]}
							<button class:on={L.s.custom_base === id} onclick={() => setBase(id)} disabled={L.running}
								>{presets[id].label}</button
							>
						{/if}
					{/each}
				</div>
				<label class="memory">
					<span>Mémoire pour le jeu : <strong>{go(L.s.custom_memory_gb ?? L.pv.resolved.memory_gb)}</strong></span>
					<input
						type="range"
						min="3"
						step="0.5"
						max={L.pv.memory_cap_gb}
						value={L.s.custom_memory_gb ?? L.pv.resolved.memory_gb}
						disabled={L.running}
						onchange={(e) => {
							if (!L.s) return;
							L.s.custom_memory_gb = Number((e.currentTarget as HTMLInputElement).value);
							L.save();
						}}
					/>
					<span class="faint">Conseillé pour ta machine : {go(L.pv.memory_auto_gb)}. Au plus {go(L.pv.memory_cap_gb)} : le reste est gardé pour le système.</span>
				</label>
			</div>
		{/if}

		<div class="panel">
			<div class="section-head">
				<div class="section-title">Réglages</div>
				{#if changed}
					<button class="link" onclick={resetAll} disabled={L.running}>Revenir aux valeurs conseillées</button>
				{/if}
			</div>
			{#each Object.entries(L.pv.file.sliders ?? {}) as [id, sl] (id)}
				{@const off = !!sl.requires && !L.pv.resolved.toggles[sl.requires]}
				<label class="slider" class:off>
					<span class="slider-head">
						<span>{sl.label} {@render tags(id, L.s.sliders[id] !== undefined)}</span>
						<strong
							title={followsScreen(id) && L.ov?.hardware.display.vrr_active
								? `Limité à ${sliderValue(id)} images/s pour rester dans la plage FreeSync / G-Sync`
								: ''}>{off ? 'Vue lointaine coupée' : sliderText(id, sl.unit, sl.max)}</strong
						>
					</span>
					<input
						type="range"
						min={sl.min}
						max={sl.max}
						step={sl.step}
						value={sliderValue(id)}
						disabled={L.running || off}
						oninput={(e) => setSlider(id, Number((e.currentTarget as HTMLInputElement).value), false)}
						onchange={(e) => setSlider(id, Number((e.currentTarget as HTMLInputElement).value), true)}
					/>
					<span class="hint">{sl.description}</span>
				</label>
			{/each}
			<p class="faint total">Vue totale : environ {totalView} autour de toi.</p>
		</div>

		{#each SECTIONS as sec (sec.id)}
			{@const items = toggles.filter(([, t]) => (t.category || 'graphismes') === sec.id)}
			{#if items.length || (sec.id === 'mods' && modGroups.length)}
				<div class="panel">
					<div class="section-title">{sec.title}</div>
					{#each items as [id, t] (id)}
						<label class="switch">
							<span>
								{t.label}
								{@render tags(id, L.s.toggles[id] !== undefined)}
								<span class="hint block">{t.description}</span>
							</span>
							<input
								type="checkbox"
								checked={L.pv.resolved.toggles[id]}
								disabled={L.running}
								onchange={(e) => setToggle(id, (e.currentTarget as HTMLInputElement).checked)}
							/>
						</label>
					{/each}
					{#if sec.id === 'mods'}
						{#each modGroups as g (g)}
							<label class="switch">
								<span>
									{L.pv.file.group_info[g].label}
									{@render tags(g, (L.s.mods ?? {})[g] !== undefined)}
									<span class="hint block">{L.pv.file.group_info[g].description}</span>
								</span>
								<input
									type="checkbox"
									checked={L.pv.resolved.groups.includes(g)}
									disabled={L.running}
									onchange={(e) => setMod(g, (e.currentTarget as HTMLInputElement).checked)}
								/>
							</label>
						{/each}
					{/if}
				</div>
			{/if}
		{/each}

		<p class="faint">
			Tout s’applique au prochain lancement du jeu. Distance, images/s, synchro ou shaders changés en jeu reviennent ici tout seuls.
			« auto » : ajusté pour ta machine.
		</p>
	{/if}
</div>

<style>
	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
	}
	h2 {
		font-family: var(--pixel);
		font-size: 22px;
		font-weight: 400;
	}
	.machine {
		margin: -6px 0 0;
	}
	.screen {
		display: block;
	}
	.cards {
		display: grid;
		/* Quatre choix, une seule ligne : jamais un orphelin sur la suivante. */
		grid-template-columns: repeat(4, minmax(0, 1fr));
		gap: 10px;
	}
	.card {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 14px;
		text-align: left;
		background: var(--panel);
		border: 2px solid #000;
		box-shadow: inset 0 0 0 1px var(--line);
		cursor: pointer;
	}
	.card:hover:not(:disabled) {
		box-shadow: inset 0 0 0 1px var(--line-strong);
		background: rgba(255, 255, 255, 0.03);
	}
	.card.on {
		box-shadow: inset 0 0 0 2px var(--yellow);
		background: rgba(245, 197, 24, 0.06);
	}
	.title {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
		font-size: 16px;
		font-weight: 700;
	}
	.desc {
		color: var(--text-dim);
		font-size: 13px;
	}
	.badge {
		font-size: 11px;
		font-weight: 700;
		padding: 1px 6px;
		background: var(--yellow);
		color: #000;
		white-space: nowrap;
	}
	.section-head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 12px;
	}
	.slider {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 10px 0;
	}
	.slider + .slider,
	.switch + .switch {
		border-top: 1px solid var(--line);
	}
	.slider.off {
		opacity: 0.55;
	}
	.slider-head {
		display: flex;
		justify-content: space-between;
		gap: 12px;
	}
	.slider-head strong {
		white-space: nowrap;
		color: var(--yellow-soft);
		font-weight: 600;
	}
	.total {
		margin: 4px 0 0;
	}
	.tag {
		margin-left: 6px;
		font-size: 11px;
		white-space: nowrap;
	}
	.tag.mine {
		color: var(--yellow-soft);
	}
	.tag.auto {
		color: #9ecbff;
		border: 1px solid #3b5a80;
		padding: 0 4px;
		cursor: help;
	}
	.block {
		display: block;
	}
	.memory {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding-top: 12px;
		margin-top: 12px;
		border-top: 1px solid var(--line);
	}
	.problem {
		box-shadow: inset 0 0 0 1px #7a2a2a;
	}
</style>
