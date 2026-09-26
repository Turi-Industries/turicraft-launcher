<script lang="ts">
	import { untrack } from 'svelte';

	// Snake, pour patienter pendant la préparation et le démarrage du jeu.
	// Affiché par Play.svelte tant que le jeu n'est pas au menu, sur toute la
	// zone au-dessus de la barre de chargement : il disparaît tout seul quand
	// le jeu est prêt. Flèches, ZQSD ou WASD (touches
	// physiques : e.code), Espace pour commencer ou reprendre.

	type P = { x: number; y: number };
	type Phase = 'idle' | 'run' | 'pause' | 'over';

	const W = 24;
	const H = 13;
	const BEST_KEY = 'turicraft.snake.best';

	let { onhide }: { onhide: () => void } = $props();

	let canvas = $state<HTMLCanvasElement | null>(null);
	let board = $state<HTMLDivElement | null>(null);
	/** Taille d'une case en pixels : la plus grande qui tient, entière (net). */
	let cell = $state(16);
	let phase = $state<Phase>('idle');
	let score = $state(0);
	let best = $state(readBest());

	let snake: P[] = [];
	let dir: P = { x: 1, y: 0 };
	let queue: P[] = [];
	let food: P = { x: 0, y: 0 };
	let timer: ReturnType<typeof setTimeout> | null = null;

	function readBest(): number {
		try {
			return Number(localStorage.getItem(BEST_KEY)) || 0;
		} catch {
			return 0;
		}
	}
	function saveBest() {
		if (score <= best) return;
		best = score;
		try {
			localStorage.setItem(BEST_KEY, String(best));
		} catch {
			/* stockage indisponible : le record vaut pour cette fois */
		}
	}

	function reset() {
		const y = Math.floor(H / 2);
		snake = [
			{ x: 6, y },
			{ x: 5, y },
			{ x: 4, y }
		];
		dir = { x: 1, y: 0 };
		queue = [];
		score = 0;
		placeFood();
	}

	function placeFood() {
		const free: P[] = [];
		for (let x = 0; x < W; x++)
			for (let y = 0; y < H; y++) if (!snake.some((s) => s.x === x && s.y === y)) free.push({ x, y });
		food = free[Math.floor(Math.random() * free.length)] ?? { x: 0, y: 0 };
	}

	/** Un peu plus rapide à chaque pomme, jusqu'à un plafond jouable. */
	const delay = () => Math.max(65, 140 - score * 3);

	function step() {
		const next = queue.shift();
		if (next) dir = next;
		const head = { x: snake[0].x + dir.x, y: snake[0].y + dir.y };
		const eats = head.x === food.x && head.y === food.y;
		const body = eats ? snake : snake.slice(0, -1);
		if (head.x < 0 || head.y < 0 || head.x >= W || head.y >= H || body.some((s) => s.x === head.x && s.y === head.y)) {
			phase = 'over';
			saveBest();
			draw();
			return;
		}
		snake = [head, ...body];
		if (eats) {
			score++;
			placeFood();
		}
		draw();
		timer = setTimeout(step, delay());
	}

	function start() {
		if (phase === 'idle' || phase === 'over') reset();
		phase = 'run';
		// Un bouton gardant le focus (Annuler…) prendrait Espace ou Entrée.
		(document.activeElement as HTMLElement | null)?.blur?.();
		if (timer) clearTimeout(timer);
		timer = setTimeout(step, delay());
	}

	function pause() {
		if (phase !== 'run') return;
		phase = 'pause';
		if (timer) clearTimeout(timer);
		timer = null;
		draw();
	}

	const DIRS: Record<string, P> = {
		ArrowUp: { x: 0, y: -1 },
		KeyW: { x: 0, y: -1 },
		ArrowDown: { x: 0, y: 1 },
		KeyS: { x: 0, y: 1 },
		ArrowLeft: { x: -1, y: 0 },
		KeyA: { x: -1, y: 0 },
		ArrowRight: { x: 1, y: 0 },
		KeyD: { x: 1, y: 0 }
	};

	function onKey(e: KeyboardEvent) {
		if (e.target instanceof HTMLInputElement) return;
		const d = DIRS[e.code];
		if (e.code === 'Space') {
			e.preventDefault();
			if (phase === 'run') pause();
			else start();
			return;
		}
		if (!d) return;
		e.preventDefault(); // pas de défilement de la page aux flèches
		if (phase !== 'run') {
			// Première touche : la partie commence dans cette direction.
			const wasIdle = phase !== 'pause';
			start();
			if (wasIdle && d.x === -1) return; // le serpent part vers la droite
		}
		const last = queue[queue.length - 1] ?? dir;
		if (queue.length < 2 && !(d.x === -last.x && d.y === -last.y) && !(d.x === last.x && d.y === last.y)) queue.push(d);
	}

	// ─── Dessin : pixels nets, couleurs du launcher ─────────────────────────
	function draw() {
		const c = canvas?.getContext('2d');
		if (!c) return;
		const CELL = cell;
		const u = CELL / 16; // les détails (pomme, yeux) sont dessinés pour 16 px
		const px = (x: number, y: number, w: number, h: number) =>
			c.fillRect(Math.round(x), Math.round(y), Math.max(1, Math.round(w)), Math.max(1, Math.round(h)));
		c.fillStyle = '#07080a';
		c.fillRect(0, 0, W * CELL, H * CELL);
		c.fillStyle = '#0d0f12';
		for (let x = 0; x < W; x++) for (let y = 0; y < H; y++) if ((x + y) % 2) c.fillRect(x * CELL, y * CELL, CELL, CELL);

		// Pomme : rouge, reflet, queue et feuille, comme celle du jeu.
		const fx = food.x * CELL;
		const fy = food.y * CELL;
		c.fillStyle = '#c62828';
		px(fx + 3 * u, fy + 5 * u, 10 * u, 9 * u);
		c.fillStyle = '#ff5a4f';
		px(fx + 4 * u, fy + 6 * u, 3 * u, 3 * u);
		c.fillStyle = '#5b3a1e';
		px(fx + 7 * u, fy + 2 * u, 2 * u, 3 * u);
		c.fillStyle = '#4caf50';
		px(fx + 9 * u, fy + 2 * u, 3 * u, 2 * u);

		snake.forEach((s, i) => {
			const x = s.x * CELL;
			const y = s.y * CELL;
			const b = Math.max(1, Math.round(u));
			c.fillStyle = '#000';
			c.fillRect(x, y, CELL, CELL);
			c.fillStyle = i === 0 ? '#ffe066' : i % 2 ? '#f5c518' : '#e0b40f';
			c.fillRect(x + b, y + b, CELL - 2 * b, CELL - 2 * b);
			c.fillStyle = 'rgba(255,255,255,0.25)';
			c.fillRect(x + b, y + b, CELL - 2 * b, 2 * b);
		});
		// Yeux, tournés vers l'avant.
		if (snake.length) {
			const h = snake[0];
			const cx = h.x * CELL + CELL / 2 + dir.x * 3 * u;
			const cy = h.y * CELL + CELL / 2 + dir.y * 3 * u;
			c.fillStyle = '#000';
			const ox = dir.y !== 0 ? 3 * u : 0;
			const oy = dir.x !== 0 ? 3 * u : 0;
			px(cx - ox - u, cy - oy - u, 2 * u, 2 * u);
			px(cx + ox - u, cy + oy - u, 2 * u, 2 * u);
		}
	}

	// La plus grande case entière qui tient dans la zone ; redessin à chaque
	// changement de taille de la fenêtre.
	$effect(() => {
		const el = board;
		if (!el) return;
		const ro = new ResizeObserver(() => {
			cell = Math.max(8, Math.floor(Math.min((el.clientWidth - 4) / W, (el.clientHeight - 4) / H)));
			requestAnimationFrame(draw);
		});
		ro.observe(el);
		return () => ro.disconnect();
	});

	$effect(() => {
		if (!canvas) return;
		// Sans suivi : draw() lit la taille des cases, et un redimensionnement
		// ne doit pas relancer la partie.
		untrack(() => {
			reset();
			draw();
		});
		const blur = () => pause(); // le jeu a pris la main : on ne perd pas la partie
		window.addEventListener('keydown', onKey);
		window.addEventListener('blur', blur);
		return () => {
			if (timer) clearTimeout(timer);
			window.removeEventListener('keydown', onKey);
			window.removeEventListener('blur', blur);
		};
	});
</script>

<section class="panel snake">
	<div class="head">
		<div class="section-title">En attendant : Snake</div>
		<div class="scores">
			<span>Pommes <strong>{score}</strong></span>
			<span>Record <strong>{best}</strong></span>
			<button class="link" onclick={onhide}>Masquer</button>
		</div>
	</div>
	<div class="area" bind:this={board}>
	<div class="board">
		<canvas bind:this={canvas} width={W * cell} height={H * cell}></canvas>
		{#if phase !== 'run'}
			<button class="overlay" onclick={start}>
				{#if phase === 'over'}
					<strong>Perdu ! {score} {score > 1 ? 'pommes' : 'pomme'}</strong>
					<span>Espace pour rejouer</span>
				{:else if phase === 'pause'}
					<strong>Pause</strong>
					<span>Espace ou une flèche pour reprendre</span>
				{:else}
					<strong>Flèches ou ZQSD</strong>
					<span>Se ferme tout seul quand le jeu est prêt</span>
				{/if}
			</button>
		{/if}
	</div>
	</div>
</section>

<style>
	.snake {
		flex: 1;
		min-height: 0;
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 12px 14px 14px;
	}
	.head .section-title {
		margin-bottom: 0;
	}
	/* Toute la place restante ; le plateau s'y centre. */
	.area {
		flex: 1;
		min-height: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		overflow: hidden;
	}
	.head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 12px;
	}
	.scores {
		display: flex;
		align-items: baseline;
		gap: 14px;
		color: var(--text-dim);
		font-size: 13px;
		white-space: nowrap;
	}
	.scores strong {
		color: var(--text);
	}
	.board {
		position: relative;
		width: fit-content;
		border: 2px solid #000;
		box-shadow: 0 0 0 1px var(--line-strong);
	}
	canvas {
		display: block;
	}
	.overlay {
		position: absolute;
		inset: 0;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 4px;
		background: rgba(0, 0, 0, 0.55);
		border: none;
		cursor: pointer;
		color: var(--text-dim);
	}
	.overlay strong {
		font-family: var(--pixel);
		font-size: 22px;
		font-weight: 400;
		color: var(--yellow);
		text-shadow: 2px 2px 0 #000;
	}
</style>
