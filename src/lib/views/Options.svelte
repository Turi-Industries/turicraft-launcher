<script lang="ts">
	import { api, type Behavior } from '$lib/api';
	import { L } from '$lib/state.svelte';

	const BEHAVIORS: { id: Behavior; label: string; hint: string }[] = [
		{ id: 'reduire', label: 'Réduire le launcher', hint: 'Il revient tout seul quand le jeu se ferme.' },
		{ id: 'garder', label: 'Le laisser ouvert', hint: 'Pratique pour suivre le journal.' },
		{ id: 'fermer', label: 'Le fermer', hint: 'Il se rouvre seulement si le jeu plante, pour montrer le rapport.' }
	];

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
		<div class="section-title">Launcher</div>
		<div class="repair">
			<div>
				<strong>Version {L.ov?.launcher_version ?? '…'}</strong>
				<div class="hint">
					{#if L.updating}Téléchargement… le launcher redémarre tout seul ensuite.
					{:else if L.updateCheck === 'checking'}Recherche en cours…
					{:else if L.launcherUpdate && L.updateLater === 'ready'}Version <strong>{L.launcherUpdate.version}</strong> prête : elle s’installe quand tu fermes le launcher.
					{:else if L.launcherUpdate && L.updateLater === 'downloading'}Version {L.launcherUpdate.version} : téléchargement en arrière-plan…
					{:else if L.launcherUpdate}Nouvelle version disponible : <strong>{L.launcherUpdate.version}</strong>
					{:else if L.updateCheck === 'uptodate'}Tu as la dernière version.
					{:else if L.updateCheck === 'error'}Impossible de vérifier pour l’instant : {L.updateError}
					{:else}Le launcher vérifie aussi tout seul à chaque ouverture.{/if}
				</div>
				{#if L.updating}
					<div class="bar" class:indeterminate={L.updating.total === 0}>
						<div style="width: {L.updating.total ? (L.updating.done / L.updating.total) * 100 : 0}%"></div>
					</div>
				{/if}
			</div>
			{#if L.launcherUpdate}
				<button class="mc-btn small" onclick={() => L.installLauncherUpdate()} disabled={!!L.updating || L.running}
					>{L.updateLater === 'ready' ? 'Redémarrer maintenant' : 'Mettre à jour'}</button
				>
			{:else}
				<button class="mc-btn small" onclick={() => L.checkLauncherUpdate()} disabled={L.updateCheck === 'checking'}
					>Rechercher une mise à jour</button
				>
			{/if}
		</div>
	</div>

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
			<div class="grow">
				<strong>Réparer le jeu</strong>
				<div class="hint">
					{#if L.repairing}{L.stage}… chaque fichier est relu et remplacé s’il est abîmé.
					{:else if L.repairDone}Installation vérifiée : tout est en ordre.
					{:else}Relit chaque fichier du jeu et du pack, et remplace ceux qui sont abîmés. Quelques minutes.{/if}
				</div>
				{#if L.repairing}
					<div class="bar" class:indeterminate={L.progress.total === 0}>
						<div style="width: {L.progress.total ? (L.progress.done / L.progress.total) * 100 : 0}%"></div>
					</div>
				{:else if L.error && !L.repairDone}
					<div class="hint problem-text">{L.error}</div>
				{/if}
			</div>
			{#if L.repairing}
				<button class="mc-btn small" onclick={() => L.stop()}>Arrêter</button>
			{:else}
				<button class="mc-btn small" disabled={L.running || !!L.updating} onclick={() => L.repair()}>Réparer</button>
			{/if}
		</div>
		<div class="repair sep">
			<div class="grow">
				<strong>Réparer le launcher</strong>
				<div class="hint">
					{#if L.reinstalling}Téléchargement… le launcher se réinstalle puis redémarre.
					{:else if L.updateError && !L.launcherUpdate}{L.updateError}
					{:else}Réinstalle la dernière version du launcher. Ton compte, tes réglages et le jeu sont gardés.{/if}
				</div>
				{#if L.reinstalling && L.updating}
					<div class="bar" class:indeterminate={L.updating.total === 0}>
						<div style="width: {L.updating.total ? (L.updating.done / L.updating.total) * 100 : 0}%"></div>
					</div>
				{/if}
			</div>
			<button class="mc-btn small" disabled={L.running || !!L.updating} onclick={() => L.reinstallLauncher()}
				>Réparer le launcher</button
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
	.repair .grow {
		flex: 1;
		min-width: 0;
	}
	.repair.sep {
		border-top: 1px solid var(--line);
		margin-top: 12px;
		padding-top: 12px;
	}
	.problem-text {
		color: var(--red, #e06c5a);
		white-space: pre-line;
	}
	.bar {
		height: 8px;
		margin-top: 6px;
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
	.about {
		user-select: text;
		line-height: 1.6;
		padding-bottom: 8px;
	}
</style>
