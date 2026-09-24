<script lang="ts">
	import { api, type Behavior } from '$lib/api';
	import { L } from '$lib/state.svelte';

	const BEHAVIORS: { id: Behavior; label: string; hint: string }[] = [
		{ id: 'reduire', label: 'Réduire le launcher', hint: 'Il revient tout seul quand le jeu se ferme.' },
		{ id: 'garder', label: 'Le laisser ouvert', hint: 'Pratique pour suivre le journal.' },
		{ id: 'fermer', label: 'Le fermer', hint: 'Il se rouvre seulement si le jeu plante, pour montrer le rapport.' }
	];

	let repaired = $state(false);
</script>

<div class="page">
	<h2>Options</h2>

	{#if L.s}
		<div class="panel">
			<div class="section-title">Quand le jeu est lancé</div>
			{#each BEHAVIORS as b (b.id)}
				<label class="choice">
					<input
						type="radio"
						name="behavior"
						checked={L.s.launcher_behavior === b.id}
						onchange={() => {
							if (!L.s) return;
							L.s.launcher_behavior = b.id;
							L.save();
						}}
					/>
					<span>{b.label}<span class="hint block">{b.hint}</span></span>
				</label>
			{/each}
			<label class="switch">
				<span>
					Rejoindre directement le serveur
					<span class="hint block">Sinon, le jeu s’ouvre sur son menu principal.</span>
				</span>
				<input type="checkbox" bind:checked={L.s.join_server} onchange={() => L.save()} />
			</label>
		</div>
	{/if}

	<div class="panel">
		<div class="section-title">Dossiers</div>
		<div class="row">
			<button class="mc-btn small" onclick={() => api.openFolder('instance')}>Dossier du jeu</button>
			<button class="mc-btn small" onclick={() => api.openFolder('screenshots')}>Captures d’écran</button>
			<button class="mc-btn small" onclick={() => api.openFolder('logs')}>Journaux</button>
		</div>
	</div>

	<div class="panel">
		<div class="section-title">Dépannage</div>
		<div class="repair">
			<div>
				<strong>Réparer l’installation</strong>
				<div class="hint">Au prochain lancement, chaque fichier du pack est revérifié et remplacé s’il est abîmé.</div>
			</div>
			<button
				class="mc-btn small"
				disabled={L.running || repaired}
				onclick={() => api.repair().then(() => (repaired = true))}>{repaired ? 'Prévu' : 'Réparer'}</button
			>
		</div>
	</div>

	{#if L.ov}
		<div class="about faint">
			Launcher {L.ov.launcher_version} · pack {L.ov.pack_version ?? 'pas encore installé'}
			{#if L.ov.disk_free_gb !== null}· {L.ov.disk_free_gb.toFixed(0)} Go libres{/if}
			<br />{L.ov.data_dir}
		</div>
	{/if}
</div>

<style>
	h2 {
		font-family: var(--pixel);
		font-size: 22px;
		font-weight: 400;
	}
	.block {
		display: block;
	}
	.switch {
		border-top: 1px solid var(--line);
		margin-top: 6px;
	}
	.row {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}
	.repair {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
	}
	.about {
		user-select: text;
		line-height: 1.6;
		padding-bottom: 8px;
	}
</style>
