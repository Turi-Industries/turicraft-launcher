<script lang="ts">
	import { api } from '$lib/api';
	import { L } from '$lib/state.svelte';

	const preset = $derived(L.pv ? L.pv.file.presets[L.pv.resolved.preset] : null);
	const serverUp = $derived(!!L.server?.online && (L.server?.max_players ?? 0) > 0);
	const date = (d: string) =>
		new Date(d + 'T12:00:00').toLocaleDateString('fr-FR', { day: 'numeric', month: 'long' });
</script>

<div class="play">
	<div class="scroll">
		<section class="hero">
			<div class="hero-text">
				<h1>Turi Craft</h1>
				<p>Turi Craft V2{L.ov?.pack_version ? ` · pack ${L.ov.pack_version}` : ''}</p>
			</div>
			<div class="server" title={L.server?.error ?? ''}>
				<span class="dot" class:on={serverUp}></span>
				<div>
					{#if !L.server}
						<strong>Serveur</strong><span class="hint">Recherche…</span>
					{:else if serverUp}
						<strong>{L.server.players} / {L.server.max_players} joueurs</strong>
						<span class="hint">{L.server.motd || 'En ligne'} · {L.server.latency_ms} ms</span>
					{:else}
						<strong>Serveur hors ligne</strong><span class="hint">Réessai toutes les 30 s</span>
					{/if}
				</div>
			</div>
		</section>

		{#if L.loginMode === 'navigateur'}
			<section class="panel device">
				<div class="section-title">Connexion Microsoft</div>
				<p>Choisis ton compte dans la page qui vient de s’ouvrir dans ton navigateur. Le launcher se connecte tout seul ensuite.</p>
				<div class="row">
					<button class="mc-btn small" onclick={() => L.login()}>Rouvrir la page</button>
					<button class="link" onclick={() => L.loginWithCode()}>Utiliser un code à la place</button>
					<button class="link" onclick={() => L.cancelLogin()}>Annuler</button>
				</div>
			</section>
		{:else if L.device}
			<section class="panel device">
				<div class="section-title">Connexion par code</div>
				<p>Sur la page qui vient de s’ouvrir, saisis ce code :</p>
				<div class="code">{L.device.user_code}</div>
				<div class="row">
					<button class="mc-btn small" onclick={() => L.device && api.openUrl(L.device.verification_uri)}
						>Ouvrir microsoft.com/link</button
					>
					<span class="hint">{L.copied ? 'Code copié.' : ''}</span>
					<button class="link" onclick={() => L.cancelLogin()}>Annuler</button>
				</div>
			</section>
		{/if}
		{#if L.loginError}
			<section class="panel problem">
				<p><strong>Connexion impossible.</strong> {L.loginError}</p>
				<div class="row">
					<button class="mc-btn small" onclick={() => L.login()}>Réessayer</button>
					<button class="link" onclick={() => L.loginWithCode()}>Utiliser un code à la place</button>
				</div>
			</section>
		{/if}

		{#if L.launcherUpdate}
			<section class="panel notice">
				<div class="grow">
					<strong>Nouveau launcher : {L.launcherUpdate.version}</strong>
					{#if L.launcherUpdate.notes}<div class="hint">{L.launcherUpdate.notes}</div>{/if}
					{#if L.updating}
						<div class="bar small-bar" class:indeterminate={L.updating.total === 0}>
							<div style="width: {L.updating.total ? (L.updating.done / L.updating.total) * 100 : 0}%"></div>
						</div>
						<div class="faint">Téléchargement… le launcher redémarre tout seul ensuite.</div>
					{/if}
					{#if L.updateError}<div class="hint">{L.updateError}</div>{/if}
				</div>
				<button class="mc-btn small" onclick={() => L.installLauncherUpdate()} disabled={!!L.updating || L.running}
					>Mettre à jour</button
				>
			</section>
		{/if}

		{#if L.error}
			<section class="panel problem">
				<div class="section-title">Ça n’a pas marché</div>
				<pre>{L.error}</pre>
				<div class="row">
					<button class="mc-btn small" onclick={() => (L.view = 'journal')}>Voir le journal</button>
					<button class="link" onclick={() => (L.error = null)}>Masquer</button>
				</div>
			</section>
		{/if}

		{#if L.crash}
			<section class="panel problem">
				<div class="section-title">Le jeu a planté</div>
				<p><strong>{L.crash.description}</strong></p>
				<pre>{L.crash.cause}</pre>
				{#if L.crash.first_error}
					<p class="hint">Première erreur du journal — souvent le vrai coupable :</p>
					<pre>{L.crash.first_error}</pre>
				{/if}
				{#if L.crash.cascade}
					<p class="hint">Un mod a échoué plus tôt : les erreurs suivantes en découlent.</p>
				{/if}
				<div class="row">
					<button class="mc-btn small" onclick={() => api.openFolder('crash')}>Rapports de crash</button>
					<button class="mc-btn small" onclick={() => api.openFolder('logs')}>Journaux du jeu</button>
					<button class="link" onclick={() => (L.crash = null)}>Masquer</button>
				</div>
			</section>
		{/if}

		{#if L.news.length}
			<section class="news">
				<div class="section-title">Nouveautés</div>
				{#each L.news as n (n.title)}
					<article>
						<div class="date">{date(n.date)}</div>
						<h3>{n.title}</h3>
						{#if n.body}<p>{n.body}</p>{/if}
						{#if n.url?.startsWith('https://')}
							<button class="link" onclick={() => n.url && api.openUrl(n.url)}>Lire sur Discord</button>
						{/if}
					</article>
				{/each}
			</section>
		{/if}
	</div>

	<footer class="dock">
		{#if L.running}
			<div class="progress">
				<div class="line">
					<span class="stage">{L.inGame ? 'En jeu' : L.milestone ? L.milestone.label : L.stage}</span>
					{#if !L.milestone && L.progress.total > 0}<span class="hint">{L.progress.done} / {L.progress.total}</span>{/if}
				</div>
				<div class="bar" class:indeterminate={!L.milestone && !L.inGame && L.progress.total === 0}>
					<div style="width: {L.percent}%"></div>
				</div>
				<div class="faint detail">
					{#if L.inGame}Le launcher reste disponible pour arrêter le jeu s’il ne répond plus.
					{:else if !L.milestone}{L.lastLog}{/if}
				</div>
				{#if !L.inGame}
					<div class="warn">⚠ Ça peut être long, surtout la première fois (plus de 400 mods).</div>
				{/if}
			</div>
			<button class="mc-btn danger" onclick={() => L.stop()}>{L.inGame || L.milestone ? 'Arrêter le jeu' : 'Annuler'}</button>
		{:else}
			<div class="summary">
				{#if preset}
					<button class="chip" onclick={() => (L.view = 'qualite')} title="Changer la qualité">
						Qualité <strong>{L.s?.preset === 'personnalise' ? 'Avancée' : preset.label}</strong> · {L.pv?.resolved.memory_gb} Go
					</button>
				{/if}
				<div class="faint">
					{#if !L.playerName}Avec ton compte Microsoft.
					{:else if L.s?.join_server}Rejoint directement le serveur.
					{:else}Ouvre le menu du jeu.{/if}
					{#if L.updates?.pack_online && L.updates.pack_installed && L.updates.pack_online !== L.updates.pack_installed}
						Mise à jour {L.updates.pack_online} installée au lancement.
					{/if}
				</div>
			</div>
			{#if L.playerName}
				<button class="mc-btn play-btn" onclick={() => L.play()} disabled={!L.canPlay}>Jouer</button>
			{:else}
				<!-- Sans compte, l'action principale est de se connecter. -->
				<button class="mc-btn play-btn login" onclick={() => L.login()} disabled={!!L.loginMode}>Se connecter</button>
			{/if}
		{/if}
	</footer>
</div>

<style>
	.play {
		display: grid;
		/* Colonne bornée : sans elle, une longue ligne du journal (un chemin
		   de l'installeur NeoForge) élargit tout l'écran et pousse le bouton
		   Annuler hors de la fenêtre. */
		grid-template-columns: minmax(0, 1fr);
		grid-template-rows: 1fr auto;
		height: 100%;
		min-height: 0;
	}
	.scroll {
		container-type: inline-size;
		overflow: auto;
		padding: 20px 24px;
		display: flex;
		flex-direction: column;
		gap: 16px;
	}

	/* Bannière : une scène de nuit en dégradés (aucune image redistribuée). */
	.hero {
		position: relative;
		display: flex;
		align-items: flex-end;
		justify-content: space-between;
		gap: 16px;
		min-height: 170px;
		padding: 20px;
		border: 2px solid #000;
		background:
			linear-gradient(180deg, rgba(0, 0, 0, 0) 45%, rgba(0, 0, 0, 0.75)),
			repeating-linear-gradient(90deg, #1b2a1f 0 24px, #1e2f22 24px 48px) bottom / 100% 34px no-repeat,
			radial-gradient(ellipse at 75% 20%, rgba(245, 197, 24, 0.25), transparent 45%),
			linear-gradient(180deg, #16213e 0%, #2a3a5e 55%, #3b3350 100%);
	}
	.hero h1 {
		font-family: var(--pixel);
		font-size: 34px;
		font-weight: 700;
		letter-spacing: 1px;
		text-shadow: 3px 3px 0 #000;
		white-space: nowrap;
	}
	.hero p {
		margin: 4px 0 0;
		color: #d9dbe0;
		text-shadow: 1px 1px 0 #000;
		white-space: nowrap;
	}
	.hero-text {
		min-width: 0;
	}
	/* Fenêtre étroite : la carte du serveur passe sous le titre. */
	@container (max-width: 660px) {
		.hero {
			flex-direction: column;
			align-items: stretch;
		}
		.server {
			max-width: none;
		}
	}
	.server {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 8px 12px;
		background: var(--panel-strong);
		border: 2px solid #000;
		max-width: 50%;
	}
	.server div {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}
	.server .hint {
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.dot {
		width: 10px;
		height: 10px;
		flex: none;
		background: var(--red);
		box-shadow: 0 0 0 2px #000;
	}
	.dot.on {
		background: var(--green);
	}

	.device .code {
		font-family: var(--pixel);
		font-size: 32px;
		color: var(--yellow);
		letter-spacing: 4px;
		margin: 4px 0 8px;
		user-select: text;
	}
	.device p {
		margin: 0;
	}
	.problem {
		border-color: #5a1f1f;
		box-shadow: inset 0 0 0 1px #7a2a2a;
	}
	.problem p {
		margin: 8px 0;
	}
	.problem pre {
		max-height: 140px;
		overflow: auto;
		margin: 6px 0;
	}
	.notice {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		box-shadow: inset 0 0 0 1px #6b5a14;
	}
	.grow {
		flex: 1;
		min-width: 0;
	}
	.small-bar {
		height: 8px;
		margin-top: 8px;
	}
	.row {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 10px;
		margin-top: 10px;
	}

	.news article {
		padding: 12px 0;
		border-top: 1px solid var(--line);
	}
	.news article:first-of-type {
		border-top: none;
		padding-top: 0;
	}
	.news .date {
		color: var(--text-faint);
		font-size: 12px;
	}
	.news h3 {
		font-size: 15px;
		margin: 2px 0 4px;
	}
	.news p {
		margin: 0;
		color: var(--text-dim);
		white-space: pre-line; /* retours à la ligne des messages Discord */
	}
	.news .link {
		margin-top: 4px;
		font-size: 13px;
	}

	/* Barre du bas : l'action principale, toujours visible. */
	.dock {
		display: flex;
		align-items: center;
		gap: 20px;
		padding: 14px 24px;
		background: var(--panel-strong);
		border-top: 2px solid var(--yellow);
	}
	.summary,
	.progress {
		flex: 1 1 0;
		min-width: 0;
	}
	.summary .faint {
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.chip {
		background: none;
		border: 1px solid var(--line-strong);
		color: var(--text-dim);
		padding: 4px 10px;
		cursor: pointer;
		white-space: nowrap;
		margin-bottom: 4px;
	}
	.chip strong {
		color: var(--text);
	}
	.chip:hover {
		border-color: var(--yellow);
		color: var(--text);
	}
	.play-btn {
		/* Taille fixe : ne s'étire jamais (WebKitGTK l'étirait sur toute la
		   largeur, 24/09). */
		flex: none;
		width: 220px;
		min-height: 56px;
		font-family: var(--pixel);
		font-size: 24px;
		letter-spacing: 1px;
		text-shadow: 2px 2px 0 #333;
	}
	.play-btn.login {
		font-size: 16px;
	}
	.progress .line {
		display: flex;
		align-items: baseline;
		gap: 10px;
		margin-bottom: 6px;
	}
	.stage {
		font-weight: 600;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.bar {
		height: 12px;
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
		animation: slide 1.2s ease-in-out infinite;
	}
	@keyframes slide {
		from {
			transform: translateX(-100%);
		}
		to {
			transform: translateX(340%);
		}
	}
	.detail,
	.warn {
		margin-top: 4px;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		min-height: 18px;
	}
	.warn {
		margin-top: 2px;
		font-size: 12px;
		color: #c9a227;
	}
</style>
