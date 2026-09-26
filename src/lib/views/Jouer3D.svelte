<script lang="ts">
	// « Jouer » en vrai relief (three.js) : un cube par pixel de la police
	// Silkscreen, comme des blocs Minecraft, qui tourne sur lui-même — de dos,
	// le texte se lit à l'envers. Sans WebGL, le texte reste à plat.
	import { untrack } from 'svelte';
	import {
		AmbientLight,
		BoxGeometry,
		DirectionalLight,
		InstancedMesh,
		MeshLambertMaterial,
		Object3D,
		PerspectiveCamera,
		Scene,
		WebGLRenderer
	} from 'three';

	let {
		hot = false,
		disabled = false,
		angle = null
	}: {
		/** Survol : faces jaunes, comme le texte des boutons Minecraft. */
		hot?: boolean;
		disabled?: boolean;
		/** Aperçu : figé sous cet angle (degrés), pour les captures. */
		angle?: number | null;
	} = $props();

	// « JOUER » en Silkscreen gras à 8 px : un caractère = un pixel de la
	// police (relevé avec FreeType sur silkscreen-latin-700-normal.woff).
	const GLYPHS = [
		'...##...###...##.##..####..####',
		'...##..##.##..##.##..##....##.##',
		'...##..##.##..##.##..####..####',
		'##.##..##.##..##.##..##....####',
		'.###....###....###...####..##.##'
	];
	const TURN_MS = 5000;
	const DEPTH = 1.6; // épaisseur d'un bloc, en pixels de la police

	const COLORS = {
		face: { idle: '#ffffff', hot: '#ffe066', off: '#8a8a8a' },
		side: { idle: '#2a2a2a', hot: '#6b5410', off: '#262626' }
	};

	let canvas = $state<HTMLCanvasElement | null>(null);
	/** Premier rendu fait : le texte à plat s'efface. */
	let ready = $state(false);
	let face: MeshLambertMaterial | null = null;
	let side: MeshLambertMaterial | null = null;

	$effect(() => {
		const k = disabled ? 'off' : hot ? 'hot' : 'idle';
		face?.color.set(COLORS.face[k]);
		side?.color.set(COLORS.side[k]);
	});

	// La scène ne dépend que du canevas : couleurs (effet ci-dessus) et angle
	// sont lus sans suivi, sinon chaque survol la recréerait.
	$effect(() => {
		const el = canvas;
		if (!el) return;
		return untrack(() => scene3d(el));
	});

	function scene3d(el: HTMLCanvasElement): (() => void) | undefined {
		let renderer: WebGLRenderer;
		try {
			renderer = new WebGLRenderer({ canvas: el, alpha: true, antialias: true });
		} catch {
			return; // pas de WebGL : le texte à plat reste
		}
		renderer.setClearColor(0x000000, 0);
		renderer.setPixelRatio(window.devicePixelRatio || 1);

		const scene = new Scene();
		scene.add(new AmbientLight(0xffffff, 1.6));
		const sun = new DirectionalLight(0xffffff, 2.2);
		sun.position.set(-0.6, 1, 1.4);
		scene.add(sun);

		// Ordre des faces d'un cube : +x, −x, +y, −y, +z (avant), −z (arrière).
		const k = disabled ? 'off' : hot ? 'hot' : 'idle';
		face = new MeshLambertMaterial({ color: COLORS.face[k] });
		side = new MeshLambertMaterial({ color: COLORS.side[k] });
		const box = new BoxGeometry(1, 1, DEPTH);
		const cells: [number, number][] = [];
		GLYPHS.forEach((row, y) => [...row].forEach((c, x) => c === '#' && cells.push([x, y])));
		const mesh = new InstancedMesh(box, [side, side, side, side, face, face], cells.length);
		const w = Math.max(...GLYPHS.map((r) => r.length));
		const h = GLYPHS.length;
		const o = new Object3D();
		cells.forEach(([x, y], i) => {
			o.position.set(x - w / 2 + 0.5, h / 2 - y - 0.5, 0);
			o.updateMatrix();
			mesh.setMatrixAt(i, o.matrix);
		});
		scene.add(mesh);

		// Petit angle, caméra loin : un peu de perspective, sans que la
		// lettre la plus proche ne déborde du bouton en tournant.
		const camera = new PerspectiveCamera(12, 1, 0.1, 1000);
		const fit = () => {
			const cw = el.clientWidth || 1;
			const ch = el.clientHeight || 1;
			renderer.setSize(cw, ch, false);
			camera.aspect = cw / ch;
			// Le texte occupe 62 % de la largeur du bouton.
			const tan = Math.tan(((camera.fov / 2) * Math.PI) / 180);
			camera.position.set(0, 0, w / 0.62 / (2 * tan * camera.aspect) + DEPTH);
			camera.updateProjectionMatrix();
		};
		fit();
		const ro = new ResizeObserver(fit);
		ro.observe(el);

		const still = angle !== null || matchMedia('(prefers-reduced-motion: reduce)').matches;
		const t0 = performance.now();
		let raf = 0;
		const frame = (now: number) => {
			mesh.rotation.y = still ? ((angle ?? 20) * Math.PI) / 180 : ((now - t0) / TURN_MS) * Math.PI * 2;
			renderer.render(scene, camera);
			ready = true;
			if (!still) raf = requestAnimationFrame(frame);
		};
		raf = requestAnimationFrame(frame);

		return () => {
			cancelAnimationFrame(raf);
			ro.disconnect();
			box.dispose();
			face?.dispose();
			side?.dispose();
			face = side = null;
			renderer.dispose();
			renderer.forceContextLoss();
		};
	}
</script>

<canvas bind:this={canvas} aria-hidden="true"></canvas>
{#if !ready}<span class="flat" aria-hidden="true">Jouer</span>{/if}

<style>
	canvas {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		pointer-events: none;
	}
	.flat {
		text-shadow: 2px 2px 0 #333;
	}
</style>
