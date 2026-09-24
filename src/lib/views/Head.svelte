<script lang="ts">
	// Tête du joueur découpée dans son vrai skin (64×64) : visage 8×8 en
	// (8, 8), calque du chapeau en (40, 8) par-dessus. Sans skin : l'initiale.
	let { skin = null, name = '?', size = 40 }: { skin?: string | null; name?: string; size?: number } = $props();

	const scale = $derived(size / 8);
	const layer = (x: number) => `url(${skin}) ${-x * scale}px ${-8 * scale}px / ${64 * scale}px no-repeat`;
</script>

{#if skin}
	<div class="head" style="width:{size}px;height:{size}px;background:{layer(40)},{layer(8)}" role="img" aria-label={name}></div>
{:else}
	<div class="head initial" style="width:{size}px;height:{size}px;font-size:{size * 0.45}px">{name[0] ?? '?'}</div>
{/if}

<style>
	.head {
		flex: none;
		border: 2px solid #000;
		box-sizing: content-box;
		image-rendering: pixelated;
		background-color: #2b2d31;
	}
	.initial {
		display: grid;
		place-items: center;
		font-family: var(--pixel);
		color: var(--yellow);
	}
</style>
