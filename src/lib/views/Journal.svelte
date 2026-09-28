<script lang="ts">
	import { tick } from 'svelte';
	import { api, REPORT_REASONS, type ManualReport } from '$lib/api';
	import { isPreview } from '$lib/preview';
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
		void L.logSeq;
		if (follow) tick().then(() => box && (box.scrollTop = box.scrollHeight));
	});

	function onScroll() {
		if (box) follow = box.scrollHeight - box.scrollTop - box.clientHeight < 24;
	}

	function toBottom() {
		follow = true;
		box?.scrollTo({ top: box.scrollHeight });
	}

	// « Envoyer un rapport » : une déconnexion, un gel ou un bug en jeu ne
	// ferment pas le jeu en erreur, et aucun rapport ne part tout seul.
	const signal = isPreview ? new URLSearchParams(location.search).get('signal') : null;
	let report = $state<ManualReport | null>(null);
	let reportOpen = $state(signal === 'ouvert' || signal === 'envoi' || signal === 'echec');
	let reason = $state<string | null>(signal === 'envoi' || signal === 'echec' ? 'deconnexion' : null);
	let sending = $state(false);
	let sentId = $state<string | null>(null);
	let sendError = $state<string | null>(null);

	async function refreshReport() {
		try {
			report = await api.manualReportStatus();
		} catch {
			report = null;
		}
	}

	// Au chargement de l'écran, et quand le jeu démarre ou s'arrête : le
	// journal du jeu change alors.
	$effect(() => {
		void L.running;
		refreshReport();
	});

	function openReport() {
		reportOpen = true;
		sentId = null;
		sendError = null;
		refreshReport();
	}

	async function sendReport() {
		if (!reason || sending) return;
		sending = true;
		sendError = null;
		try {
			sentId = await api.manualReportSend(reason);
			reportOpen = false;
			reason = null;
			await refreshReport();
		} catch (e) {
			sendError = e instanceof Error ? e.message : String(e);
		} finally {
			sending = false;
		}
	}

	$effect(() => {
		if (signal === 'envoi' && reportOpen && !sending) sendReport();
		if (signal === 'echec' && reportOpen && !sending && !sendError) sendReport();
	});

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

	<section class="panel report">
		{#if reportOpen}
			<div class="report-head">
				<strong>Ce qui s’est passé</strong>
				<button class="link" onclick={() => (reportOpen = false)} disabled={sending}>Annuler</button>
			</div>
			<div class="segmented reasons">
				{#each REPORT_REASONS as r (r.id)}
					<button class:on={reason === r.id} onclick={() => (reason = r.id)} disabled={sending}>{r.label}</button>
				{/each}
			</div>
			<div class="report-foot">
				<span class="hint"
					>{#if sendError}Rapport non envoyé : {sendError}{:else}Part : journal du jeu, machine, réglages.{/if}</span
				>
				<button class="mc-btn small" onclick={sendReport} disabled={!reason || sending}
					>{sending ? 'Envoi…' : sendError ? 'Réessayer' : 'Envoyer'}</button
				>
			</div>
		{:else}
			<div class="grow">
				{#if sentId}
					<strong>Rapport envoyé : n° {sentId}</strong>
					<div class="hint">Donne ce numéro sur Discord en expliquant ce qui s’est passé.</div>
				{:else if report?.sent_id}
					<strong>Déjà envoyé : n° {report.sent_id}</strong>
					<div class="hint">Le journal n’a pas changé depuis. Donne ce numéro sur Discord.</div>
				{:else if report && !report.has_log}
					<strong>Un souci en jeu ?</strong>
					<div class="hint">Pas encore de journal du jeu : lance le jeu une fois.</div>
				{:else}
					<strong>Un souci en jeu ?</strong>
					<div class="hint">Déconnexion, jeu figé, bug : préviens l’équipe.</div>
				{/if}
			</div>
			<button
				class="mc-btn small"
				onclick={openReport}
				disabled={!report?.has_log || !!report?.sent_id}
				title={report?.sent_id ? `Déjà envoyé : n° ${report.sent_id}` : undefined}>Envoyer un rapport</button
			>
		{/if}
	</section>

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
		overflow-x: hidden;
	}
	header {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: 10px 16px;
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
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
	}
	.report {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: 8px 16px;
		padding: 10px 14px;
		flex: none;
	}
	.report:has(.reasons) {
		flex-direction: column;
		align-items: stretch;
		gap: 10px;
	}
	.grow {
		flex: 1 1 200px;
		min-width: 0;
	}
	.report-head,
	.report-foot {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
	}
	.reasons {
		align-self: flex-start;
		flex-wrap: wrap;
	}
	.reasons button {
		padding: 5px 12px;
		font-size: 13px;
	}
	.bar {
		display: flex;
		flex-wrap: wrap;
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
		min-height: 180px;
		min-width: 0;
		display: flex;
	}
	.log {
		flex: 1;
		min-width: 0;
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
