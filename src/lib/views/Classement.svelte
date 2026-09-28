<script lang="ts" module>
	/** Têtes déjà chargées pendant cette ouverture du launcher. */
	const skins: Record<string, string | null> = {};
</script>

<script lang="ts">
	// Classement complet du Snake : le meilleur score de chaque joueur,
	// pseudos vérifiés par Mojang (snake.rs, server/snake-scores.py). Le Snake
	// se joue pendant le lancement du jeu ; ici, on regarde qui mène.
	import { onMount } from 'svelte';
	import { api, type SnakeEntry, type SnakeRanking } from '$lib/api';
	import { L } from '$lib/state.svelte';
	import Head from './Head.svelte';

	let data = $state<SnakeRanking | null>(null);
	let error = $state<string | null>(null);
	let loading = $state(false);

	/** Têtes des joueurs, gardées d'une visite à l'autre (et 6 h sur le
	 *  disque, côté cœur) : `null` = pas de skin, on garde l'initiale. */
	const heads = $state<Record<string, string | null>>({ ...skins });

	async function load() {
		loading = true;
		try {
			data = await api.snakeRanking();
			error = null;
			loadHeads(data.entries);
		} catch (e) {
			error = String(e);
		} finally {
			loading = false;
		}
	}

	/** Trois à la fois : Mojang limite son service de profils. */
	async function loadHeads(entries: SnakeEntry[]) {
		const todo = entries.map((e) => e.uuid).filter((u) => !(u in heads));
		const worker = async () => {
			for (let u = todo.shift(); u; u = todo.shift()) {
				heads[u] = await api.playerSkin(u).catch(() => null);
				skins[u] = heads[u];
			}
		};
		await Promise.all([worker(), worker(), worker()]);
	}

	onMount(load);

	const myUuid = $derived(L.s?.account?.uuid.replace(/-/g, '').toLowerCase() ?? null);
	const isMe = (r: SnakeEntry) => !!myUuid && r.uuid.replace(/-/g, '').toLowerCase() === myUuid;
	const entries = $derived(data?.entries ?? []);
	const mine = $derived(entries.findIndex(isMe));
	const podium = $derived(entries.slice(0, 3));
	const rest = $derived(entries.slice(3));

	const place = (i: number) => (i === 0 ? '1re' : `${i + 1}e`);
	const apples = (n: number) => `${n} ${n > 1 ? 'pommes' : 'pomme'}`;
	const date = (d: string) =>
		new Date(d + 'T12:00:00').toLocaleDateString('fr-FR', { day: 'numeric', month: 'short' });
	const MEDALS = ['or', 'argent', 'bronze'];
</script>

<div class="page">
	<header>
		<div>
			<h2>Classement</h2>
			<p class="hint">
				Le Snake du lancement : meilleur score de chaque joueur.
			</p>
		</div>
		<button class="mc-btn small" onclick={load} disabled={loading}>{loading ? 'Chargement…' : 'Actualiser'}</button>
	</header>

	{#if error && !data}
		<section class="panel problem">
			<p><strong>Classement indisponible pour l’instant.</strong></p>
			<p class="hint">{error}</p>
			<button class="mc-btn small" onclick={load} disabled={loading}>Réessayer</button>
		</section>
	{:else if !data}
		<p class="hint">Chargement…</p>
	{:else if !entries.length}
		<section class="panel empty">
			<strong>Personne encore.</strong>
			<span class="hint">La première place est à prendre : le Snake s’ouvre quand tu lances le jeu.</span>
		</section>
	{:else}
		{#if L.s?.account}
			<section class="panel mine" class:ranked={mine >= 0}>
				<Head skin={L.skin} name={L.s.account.name} size={36} />
				<div class="grow">
					{#if mine >= 0}
						<strong>{L.s.account.name} : {place(mine)}{data.complete ? ` sur ${entries.length}` : ''}</strong>
						<div class="hint">
							Record : {apples(entries[mine].score)}, le {date(entries[mine].date)}.
							{#if mine > 0}Encore {apples(entries[mine - 1].score - entries[mine].score + 1)} pour dépasser {entries[mine - 1].name}.{/if}
						</div>
					{:else if !data.complete}
						<strong>{L.s.account.name} : pas dans les 10 premiers</strong>
						<div class="hint">Bats ton record au Snake pendant le prochain lancement du jeu.</div>
					{:else}
						<strong>{L.s.account.name} : pas encore classé</strong>
						<div class="hint">Bats ton record au Snake pendant le prochain lancement du jeu.</div>
					{/if}
				</div>
			</section>
		{/if}

		<section class="podium">
			{#each podium as r, i (r.uuid)}
				<div class="step {MEDALS[i]}" class:me={isMe(r)}>
					<span class="pos">{i + 1}</span>
					<Head skin={heads[r.uuid] ?? null} name={r.name} size={44} />
					<span class="name" title={r.name}>{r.name}</span>
					<span class="score">{apples(r.score)}</span>
				</div>
			{/each}
		</section>

		{#if rest.length}
			<ol class="panel list" start="4">
				{#each rest as r, i (r.uuid)}
					<li class:me={isMe(r)}>
						<span class="pos">{i + 4}</span>
						<Head skin={heads[r.uuid] ?? null} name={r.name} size={22} />
						<span class="name" title={r.name}>{r.name}</span>
						<span class="date faint">{date(r.date)}</span>
						<span class="pts">{r.score}</span>
					</li>
				{/each}
			</ol>
		{/if}

		{#if !data.complete}
			<p class="faint">Les 10 premiers seulement pour l’instant : la liste complète arrive avec la prochaine mise à jour du serveur.</p>
		{/if}
	{/if}
</div>

<style>
	header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 16px;
	}
	h2 {
		font-family: var(--pixel);
		font-size: 22px;
		font-weight: 400;
	}
	header .hint {
		margin: 4px 0 0;
	}
	.problem p {
		margin: 0 0 8px;
	}
	.empty {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.mine {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 12px 16px;
	}
	.mine.ranked {
		border-left: 4px solid var(--yellow);
	}
	.grow {
		flex: 1;
		min-width: 0;
	}

	/* Podium : trois marches, la première au milieu et plus haute. */
	.podium {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		align-items: end;
		gap: 10px;
	}
	.step {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 6px;
		padding: 14px 10px 12px;
		background: var(--panel);
		border: 2px solid #000;
		box-shadow: inset 0 0 0 1px var(--line);
		min-width: 0;
	}
	.step.or {
		order: 2;
		padding-top: 26px;
		box-shadow:
			inset 0 0 0 1px var(--line),
			inset 0 4px 0 #f5c518;
	}
	.step.argent {
		order: 1;
		box-shadow:
			inset 0 0 0 1px var(--line),
			inset 0 4px 0 #c8ccd4;
	}
	.step.bronze {
		order: 3;
		box-shadow:
			inset 0 0 0 1px var(--line),
			inset 0 4px 0 #c7803e;
	}
	.step.me {
		background: #1d1a0c;
	}
	.step .name {
		max-width: 100%;
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.step.me .name {
		color: var(--yellow-soft);
	}
	.score {
		font-size: 13px;
		color: var(--text-dim);
		font-variant-numeric: tabular-nums;
	}

	/* Liste : 4e place et au-delà. */
	.list {
		list-style: none;
		margin: 0;
		padding: 6px;
	}
	li {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 5px 8px;
		border-left: 3px solid transparent;
	}
	li:nth-child(odd) {
		background: rgba(255, 255, 255, 0.03);
	}
	li.me {
		border-left-color: var(--yellow);
		background: #1d1a0c;
		color: var(--yellow-soft);
	}
	.name {
		min-width: 0;
	}
	li .name {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.date {
		flex: none;
		white-space: nowrap;
	}
	.pts {
		flex: none;
		min-width: 3ch;
		text-align: right;
		font-weight: 700;
		font-variant-numeric: tabular-nums;
	}

	/* Place : carré noir, médailles pour les trois premiers. */
	.pos {
		flex: none;
		width: 22px;
		height: 22px;
		display: grid;
		place-items: center;
		font-size: 11px;
		font-weight: 700;
		color: var(--text-dim);
		background: #000;
		border: 1px solid var(--line-strong);
	}
	.or .pos {
		background: #f5c518;
		border-color: #000;
		color: #000;
	}
	.argent .pos {
		background: #c8ccd4;
		border-color: #000;
		color: #000;
	}
	.bronze .pos {
		background: #c7803e;
		border-color: #000;
		color: #000;
	}
</style>
