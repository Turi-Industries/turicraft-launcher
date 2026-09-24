<script lang="ts">
	import { onMount } from 'svelte';
	import { L } from '$lib/state.svelte';
	import Sidebar from '$lib/views/Sidebar.svelte';
	import Play from '$lib/views/Play.svelte';
	import Quality from '$lib/views/Quality.svelte';
	import Options from '$lib/views/Options.svelte';
	import Journal from '$lib/views/Journal.svelte';
	import UpdateDialog from '$lib/views/UpdateDialog.svelte';

	onMount(() => L.start());
</script>

<div class="app">
	<Sidebar />
	<main>
		{#if L.view === 'jouer'}
			<Play />
		{:else if L.view === 'qualite'}
			<Quality />
		{:else if L.view === 'options'}
			<Options />
		{:else}
			<Journal />
		{/if}
	</main>
</div>
<UpdateDialog />

<!-- Mise en page de la fenêtre : dans app.css (.app, .app > main), chargée
     en premier. Le 24/09, un affichage empilé sans défilement a été vu sur
     un poste, non reproduit : la grille ne dépend plus du style de cette page. -->
