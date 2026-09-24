<script lang="ts">
	import { L, type View } from '$lib/state.svelte';
	import Head from './Head.svelte';

	const NAV: { id: View; label: string; icon: string }[] = [
		{ id: 'jouer', label: 'Jouer', icon: 'M6 4l12 8-12 8z' },
		{ id: 'qualite', label: 'Qualité', icon: 'M4 6h10M18 6h2M4 12h4M12 12h8M4 18h12M20 18h0M14 4v4M8 10v4M16 16v4' },
		{ id: 'options', label: 'Options', icon: 'M12 8a4 4 0 100 8 4 4 0 000-8zM12 2v3M12 19v3M2 12h3M19 12h3M5 5l2 2M17 17l2 2M5 19l2-2M17 7l2-2' },
		{ id: 'journal', label: 'Journal', icon: 'M5 4h14v16H5zM8 8h8M8 12h8M8 16h5' }
	];

</script>

<aside>
	<div class="brand">
		<div class="logo">TURI <span>CRAFT</span></div>
		<div class="tagline">Turi Craft V2</div>
	</div>

	<nav>
		{#each NAV as n (n.id)}
			<button class:active={L.view === n.id} onclick={() => (L.view = n.id)}>
				<svg viewBox="0 0 24 24" aria-hidden="true"><path d={n.icon} /></svg>
				{n.label}
				{#if n.id === 'jouer' && L.running}<span class="pulse" title="En cours"></span>{/if}
			</button>
		{/each}
	</nav>

	<div class="account">
		{#if L.s?.account}
			<div class="who">
				<Head skin={L.skin} name={L.s.account.name} />
				<div class="id">
					<div class="name">{L.s.account.name}</div>
					<button class="link" onclick={() => L.logout()} disabled={L.running}>Se déconnecter</button>
				</div>
			</div>
		{:else if L.ov?.offline_name}
			<div class="who">
				<Head name={L.ov.offline_name} />
				<div class="id">
					<div class="name">{L.ov.offline_name}</div>
					<div class="faint">Mode test hors ligne</div>
				</div>
			</div>
		{:else}
			<!-- Le bouton principal « Se connecter » est dans la barre du bas. -->
			<div class="who">
				<Head name="?" />
				<div class="id">
					<div class="name">Pas connecté</div>
					<button class="link" onclick={() => L.login()} disabled={!!L.loginMode}>Se connecter</button>
				</div>
			</div>
		{/if}
	</div>
</aside>

<style>
	aside {
		display: flex;
		flex-direction: column;
		background: var(--bg-side);
		border-right: 2px solid #000;
		min-height: 0;
	}
	.brand {
		padding: 22px 18px 18px;
	}
	.logo {
		font-family: var(--pixel);
		font-size: 24px;
		letter-spacing: 1px;
		text-shadow: 3px 3px 0 #000;
		white-space: nowrap;
	}
	.logo span {
		color: var(--yellow);
	}
	.tagline {
		color: var(--text-faint);
		font-size: 12px;
		margin-top: 2px;
	}
	nav {
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding: 6px 10px;
		flex: 1;
	}
	nav button {
		display: flex;
		align-items: center;
		gap: 12px;
		width: 100%;
		padding: 10px 12px;
		background: none;
		border: none;
		border-left: 3px solid transparent;
		color: var(--text-dim);
		font-size: 15px;
		font-weight: 600;
		text-align: left;
		cursor: pointer;
		white-space: nowrap;
	}
	nav button:hover {
		color: var(--text);
		background: rgba(255, 255, 255, 0.04);
	}
	nav button.active {
		color: var(--yellow-soft);
		border-left-color: var(--yellow);
		background: rgba(245, 197, 24, 0.07);
	}
	svg {
		width: 20px;
		height: 20px;
		flex: none;
		fill: none;
		stroke: currentColor;
		stroke-width: 2;
		stroke-linecap: square;
	}
	nav button:first-child svg path {
		fill: currentColor;
	}
	.pulse {
		margin-left: auto;
		width: 8px;
		height: 8px;
		background: var(--yellow);
		animation: blink 1s steps(2) infinite;
	}
	@keyframes blink {
		50% {
			opacity: 0.2;
		}
	}
	.account {
		padding: 14px;
		border-top: 1px solid var(--line);
	}
	.who {
		display: flex;
		align-items: center;
		gap: 10px;
		min-width: 0;
	}
	.id {
		min-width: 0;
	}
	.name {
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
</style>
