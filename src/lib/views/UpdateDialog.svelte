<script lang="ts">
	import { L } from '$lib/state.svelte';

	const u = $derived(L.launcherUpdate);
</script>

<!-- Au démarrage, si une nouvelle version du launcher existe : maintenant, ou
     en fermant le launcher (téléchargée tout de suite, installée en quittant). -->
{#if L.updatePrompt && u}
	<div class="backdrop">
		<div class="dialog" role="dialog" aria-modal="true" aria-labelledby="maj-titre">
			<div class="section-title" id="maj-titre">Mise à jour du launcher</div>
			<h3>Version {u.version} disponible</h3>
			{#if u.notes}<p class="notes">{u.notes}</p>{/if}

			{#if L.updating}
				<div class="bar" class:indeterminate={L.updating.total === 0}>
					<div style="width: {L.updating.total ? (L.updating.done / L.updating.total) * 100 : 0}%"></div>
				</div>
				<p class="faint">Téléchargement… le launcher redémarre tout seul ensuite.</p>
			{:else}
				{#if L.updateError}<p class="error">{L.updateError}</p>{/if}
				<div class="actions">
					<button class="mc-btn" onclick={() => L.installLauncherUpdate()}>Redémarrer maintenant</button>
					<button class="mc-btn secondary" onclick={() => L.installLauncherUpdateLater()}>Plus tard</button>
				</div>
				<p class="faint">Plus tard : elle se télécharge maintenant et s’installe quand tu fermes le launcher.</p>
			{/if}
		</div>
	</div>
{/if}

<style>
	.backdrop {
		position: fixed;
		inset: 0;
		z-index: 50;
		display: grid;
		place-items: center;
		padding: 16px;
		background: rgba(0, 0, 0, 0.65);
	}
	.dialog {
		width: min(460px, 100%);
		padding: 20px 22px;
		background: var(--bg-side);
		border: 2px solid #000;
		border-top: 2px solid var(--yellow);
		box-shadow: 0 10px 40px rgba(0, 0, 0, 0.6);
	}
	h3 {
		margin: 6px 0 4px;
		font-size: 17px;
	}
	.notes {
		margin: 0 0 14px;
		color: var(--text-dim);
		white-space: pre-line;
	}
	.actions {
		display: flex;
		gap: 10px;
		margin-top: 4px;
	}
	.actions .mc-btn {
		flex: 1 1 0;
	}
	.secondary {
		filter: brightness(0.85);
	}
	.faint {
		margin: 10px 0 0;
		font-size: 12px;
	}
	.error {
		color: var(--red);
		margin: 0 0 10px;
	}
	.bar {
		height: 12px;
		margin-top: 8px;
		background: #000;
		border: 2px solid #4b4b4b;
		overflow: hidden;
	}
	.bar > div {
		height: 100%;
		background: var(--yellow);
		transition: width 0.3s linear;
	}
	.bar.indeterminate > div {
		width: 30% !important;
	}
</style>
