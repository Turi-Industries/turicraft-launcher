<script lang="ts">
	import { tick } from 'svelte';
	import { api } from '$lib/api';
	import { L, type LogLine } from '$lib/state.svelte';

	type Filter = 'tout' | 'etapes' | 'problemes';
	let filter = $state<Filter>('tout');
	let copied = $state(false);
	let box = $state<HTMLDivElement | null>(null);
	/** Collé en bas : les nouvelles lignes restent visibles. Le joueur qui
	 *  remonte lire une ligne n'est pas ramené en bas malgré lui. */
	let follow = $state(true);

	const problems = $derived(L.logs.filter((l) => l.kind === 'error' || l.kind === 'warn').length);
	const shown = $derived(
		filter === 'etapes'
			? L.logs.filter((l) => l.kind === 'stage' || l.kind === 'step' || l.kind === 'error')
			: filter === 'problemes'
				? L.logs.filter((l) => l.kind === 'error' || l.kind === 'warn')
				: L.logs
	);

	const time = (t: number) =>
		new Date(t).toLocaleTimeString('fr-FR', { hour: '2-digit', minute: '2-digit', second: '2-digit' });
	const line = (l: LogLine) => `${time(l.t)}  ${l.text}`;

	$effect(() => {
		void shown.length;
		if (follow) tick().then(() => box && (box.scrollTop = box.scrollHeight));
	});

	function onScroll() {
		if (box) follow = box.scrollHeight - box.scrollTop - box.clientHeight < 24;
	}

	function toBottom() {
		follow = true;
		box?.scrollTo({ top: box.scrollHeight });
	}

	function copy() {
		navigator.clipboard
			?.writeText(shown.map(line).join('\n'))
			.then(() => {
				copied = true;
				setTimeout(() => (copied = false), 1500);
			})
			.catch(() => {});
	}
</script>

<div class="page">
	<header>
		<h2>Journal</h2>
		<div class="row">
			<button class="mc-btn small" onclick={copy} disabled={!shown.length}>{copied ? 'Copié' : 'Copier'}</button>
			<button class="mc-btn small" onclick={() => api.openFolder('logs')}>Journaux du jeu</button>
			<button class="mc-btn small" onclick={() => api.openFolder('crash')}>Rapports de crash</button>
		</div>
	</header>
	<p class="hint status">
		{#if L.running && !L.inGame}{L.repairing ? 'Réparation' : 'Préparation'} en cours : <strong>{L.milestone?.label ?? L.stage}</strong>
		{:else if L.inGame}Le jeu tourne.
		{:else}Ce que fait le launcher : installation, synchronisation, lancement. Le journal du jeu lui-même est dans « Journaux du jeu ».{/if}
	</p>

	<div class="bar">
		<div class="segmented">
			<button class:on={filter === 'tout'} onclick={() => (filter = 'tout')}>Tout <span class="n">{L.logs.length}</span></button>
			<button class:on={filter === 'etapes'} onclick={() => (filter = 'etapes')}>Étapes</button>
			<button class:on={filter === 'problemes'} onclick={() => (filter = 'problemes')}
				>Problèmes {#if problems}<span class="n bad">{problems}</span>{/if}</button
			>
		</div>
		<button class="link" onclick={() => (L.logs = [])} disabled={!L.logs.length}>Vider</button>
	</div>

	<div class="log-wrap">
		<div class="log" bind:this={box} onscroll={onScroll}>
			{#each shown as l, i (i)}
				<div class="l {l.kind}">
					<span class="t">{time(l.t)}</span>
					<span class="x">{l.kind === 'stage' ? l.text.replace(/^— /, '') : l.text}</span>
				</div>
			{:else}
				<div class="empty">
					{#if L.logs.length}Aucune ligne de ce type.
					{:else}
						<strong>Rien pour l’instant.</strong>
						<span>Lance le jeu : chaque étape s’affiche ici, avec son heure.</span>
					{/if}
				</div>
			{/each}
		</div>
		{#if !follow && shown.length}
			<button class="mc-btn small down" onclick={toBottom}>↓ Dernières lignes</button>
		{/if}
	</div>
</div>

<style>
	.page {
		gap: 12px;
		overflow: hidden;
	}
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
	.status {
		margin: -6px 0 0;
	}
	.status strong {
		color: var(--text);
	}
	.row {
		display: flex;
		align-items: center;
		gap: 8px;
		flex: none;
	}
	.bar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
	}
	.segmented button {
		padding: 4px 12px;
		font-size: 13px;
	}
	.n {
		display: inline-block;
		min-width: 18px;
		margin-left: 4px;
		padding: 0 4px;
		background: #000;
		color: var(--text-dim);
		font-size: 11px;
		text-align: center;
	}
	.n.bad {
		color: var(--red);
	}

	.log-wrap {
		position: relative;
		flex: 1;
		min-height: 0;
		display: flex;
	}
	.log {
		flex: 1;
		overflow: auto;
		background: #000;
		border: 2px solid #000;
		box-shadow: inset 0 0 0 1px var(--line);
		padding: 6px 0;
		font: 12px/1.55 ui-monospace, 'JetBrains Mono', monospace;
		user-select: text;
		-webkit-user-select: text;
	}
	.l {
		display: flex;
		gap: 12px;
		padding: 1px 12px;
		border-left: 3px solid transparent;
	}
	.l:hover {
		background: #111215;
	}
	.t {
		flex: none;
		color: var(--text-faint);
	}
	.x {
		min-width: 0;
		white-space: pre-wrap;
		word-break: break-word;
		color: var(--text-dim);
	}
	/* Début d'une étape : un intertitre, pour se repérer d'un coup d'œil. */
	.l.stage {
		margin-top: 8px;
		padding-top: 4px;
		padding-bottom: 4px;
		border-left-color: var(--yellow);
		background: #16140a;
	}
	.l.stage:first-child {
		margin-top: 0;
	}
	.l.stage .x {
		color: var(--yellow-soft);
		font-weight: 700;
	}
	.l.step .x {
		color: var(--green);
	}
	.l.warn {
		border-left-color: #c9a227;
	}
	.l.warn .x {
		color: #e6c35c;
	}
	.l.error {
		border-left-color: var(--red);
		background: #1d0d0d;
	}
	.l.error .x {
		color: #ff9c9c;
	}
	.empty {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 24px;
		color: var(--text-faint);
		font: 14px/1.5 var(--sans);
	}
	.empty strong {
		color: var(--text-dim);
	}
	.down {
		position: absolute;
		right: 16px;
		bottom: 12px;
	}
</style>
