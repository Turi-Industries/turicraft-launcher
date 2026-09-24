<script lang="ts">
	import { api } from '$lib/api';
	import { L } from '$lib/state.svelte';

	let copied = $state(false);

	function copy() {
		navigator.clipboard
			?.writeText(L.logs.join('\n'))
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
			<button class="mc-btn small" onclick={copy} disabled={!L.logs.length}>{copied ? 'Copié' : 'Copier'}</button>
			<button class="mc-btn small" onclick={() => api.openFolder('logs')}>Journaux du jeu</button>
			<button class="link" onclick={() => (L.logs = [])} disabled={!L.logs.length}>Vider</button>
		</div>
	</header>
	<p class="hint">Ce que fait le launcher : installation, synchronisation, lancement. Le journal du jeu lui-même est dans « Journaux du jeu ».</p>
	<pre>{L.logs.join('\n') || 'Rien pour l’instant. Lance le jeu pour voir les étapes ici.'}</pre>
</div>

<style>
	.page {
		gap: 10px;
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
	.row {
		display: flex;
		align-items: center;
		gap: 10px;
	}
	p {
		margin: 0;
	}
	pre {
		flex: 1;
		overflow: auto;
		min-height: 0;
	}
</style>
