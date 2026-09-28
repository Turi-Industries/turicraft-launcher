<script lang="ts">
	// « Jouer » en vrai relief : un cube par pixel de la police Silkscreen,
	// comme des blocs Minecraft, qui tourne sur lui-même — de dos, le texte se
	// lit à l'envers. Sans WebGL, le texte reste à plat.
	//
	// WebGL direct, sans three.js : la bibliothèque pesait 600 Ko sur les
	// 790 Ko de l'interface, lus et compilés à chaque ouverture du launcher,
	// pour une centaine de cubes. Même rendu (même caméra, même éclairage
	// Lambert que MeshLambertMaterial, mêmes conversions sRGB), comparé en
	// captures le 28/09. Au plus 60 images/s : sur un écran à 144 Hz, la
	// carte graphique ne travaille plus deux fois plus pour rien.
	import { untrack } from 'svelte';

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
	const FOV = 12; // degrés, vertical
	const MIN_FRAME_MS = 1000 / 60 - 1;

	const COLORS = {
		face: { idle: '#ffffff', hot: '#ffe066', off: '#8a8a8a' },
		side: { idle: '#2a2a2a', hot: '#6b5410', off: '#262626' }
	};
	// Lumières de la scène d'origine : ambiante 1,6, soleil 2,2 venant de
	// (−0,6 ; 1 ; 1,4).
	const AMBIENT = 1.6;
	const SUN = 2.2;
	const SUN_DIR = (() => {
		const v = [-0.6, 1, 1.4];
		const n = Math.hypot(...v);
		return v.map((x) => x / n);
	})();

	let canvas = $state<HTMLCanvasElement | null>(null);
	/** Premier rendu fait : le texte à plat s'efface. */
	let ready = $state(false);
	/** Couleurs à jour pour la scène (lues à chaque image). */
	let colorKey: 'idle' | 'hot' | 'off' = 'idle';
	let redraw: (() => void) | null = null;

	$effect(() => {
		colorKey = disabled ? 'off' : hot ? 'hot' : 'idle';
		redraw?.();
	});

	// La scène ne dépend que du canevas : couleurs (effet ci-dessus) et angle
	// sont lus sans suivi, sinon chaque survol la recréerait.
	$effect(() => {
		const el = canvas;
		if (!el) return;
		return untrack(() => scene3d(el));
	});

	/** '#rrggbb' → couleur linéaire (comme Color.set de three.js). */
	function linear(hex: string): [number, number, number] {
		const c = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
		return c.map((x) => (x < 0.04045 ? x * 0.0773993808 : Math.pow(x * 0.9478672986 + 0.0521327014, 2.4))) as [
			number,
			number,
			number
		];
	}

	/** Les cubes, en triangles : position, normale, face avant/arrière (1) ou côté (0). */
	function geometry(): { data: Float32Array; count: number; width: number } {
		const cells: [number, number][] = [];
		GLYPHS.forEach((row, y) => [...row].forEach((c, x) => c === '#' && cells.push([x, y])));
		const w = Math.max(...GLYPHS.map((r) => r.length));
		const h = GLYPHS.length;
		// Faces : normale, et deux axes qui la parcourent.
		const faces: [number[], number[], number[], number][] = [
			[[1, 0, 0], [0, 1, 0], [0, 0, 1], 0],
			[[-1, 0, 0], [0, 0, 1], [0, 1, 0], 0],
			[[0, 1, 0], [0, 0, 1], [1, 0, 0], 0],
			[[0, -1, 0], [1, 0, 0], [0, 0, 1], 0],
			[[0, 0, 1], [1, 0, 0], [0, 1, 0], 1],
			[[0, 0, -1], [0, 1, 0], [1, 0, 0], 1]
		];
		const half = [0.5, 0.5, DEPTH / 2];
		const out: number[] = [];
		for (const [x, y] of cells) {
			const center = [x - w / 2 + 0.5, h / 2 - y - 0.5, 0];
			for (const [n, u, v, isFace] of faces) {
				const corner = (su: number, sv: number) =>
					[0, 1, 2].map((k) => center[k] + (n[k] + su * u[k] + sv * v[k]) * half[k]);
				const a = corner(-1, -1);
				const b = corner(1, -1);
				const c = corner(1, 1);
				const d = corner(-1, 1);
				for (const p of [a, b, c, a, c, d]) out.push(...p, ...n, isFace);
			}
		}
		return { data: new Float32Array(out), count: out.length / 7, width: w };
	}

	const VERTEX = `
		attribute vec3 position;
		attribute vec3 normal;
		attribute float isFace;
		uniform mat4 projection;
		uniform float camZ;
		uniform float angle;
		varying vec3 vNormal;
		varying float vFace;
		void main() {
			float c = cos(angle), s = sin(angle);
			mat3 rot = mat3(c, 0.0, -s,  0.0, 1.0, 0.0,  s, 0.0, c);
			vec3 p = rot * position;
			vNormal = rot * normal;
			vFace = isFace;
			gl_Position = projection * vec4(p.x, p.y, p.z - camZ, 1.0);
		}`;
	const FRAGMENT = `
		precision mediump float;
		uniform vec3 faceColor;
		uniform vec3 sideColor;
		uniform vec3 sunDir;
		varying vec3 vNormal;
		varying float vFace;
		vec3 toSrgb(vec3 c) {
			return mix(c * 12.92, pow(c, vec3(0.41666)) * 1.055 - 0.055, step(0.0031308, c));
		}
		void main() {
			vec3 base = vFace > 0.5 ? faceColor : sideColor;
			float light = ${AMBIENT.toFixed(2)} + ${SUN.toFixed(2)} * max(dot(normalize(vNormal), sunDir), 0.0);
			vec3 lin = clamp(base * light * 0.3183099, 0.0, 1.0);
			gl_FragColor = vec4(toSrgb(lin), 1.0);
		}`;

	function scene3d(el: HTMLCanvasElement): (() => void) | undefined {
		const gl = el.getContext('webgl', { alpha: true, antialias: true, premultipliedAlpha: true });
		if (!gl) return; // pas de WebGL : le texte à plat reste

		const shader = (type: number, src: string) => {
			const s = gl.createShader(type)!;
			gl.shaderSource(s, src);
			gl.compileShader(s);
			return s;
		};
		const prog = gl.createProgram()!;
		gl.attachShader(prog, shader(gl.VERTEX_SHADER, VERTEX));
		gl.attachShader(prog, shader(gl.FRAGMENT_SHADER, FRAGMENT));
		gl.linkProgram(prog);
		if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) return;
		gl.useProgram(prog);

		const { data, count, width } = geometry();
		const buf = gl.createBuffer();
		gl.bindBuffer(gl.ARRAY_BUFFER, buf);
		gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
		const attr = (name: string, size: number, offset: number) => {
			const loc = gl.getAttribLocation(prog, name);
			gl.enableVertexAttribArray(loc);
			gl.vertexAttribPointer(loc, size, gl.FLOAT, false, 28, offset * 4);
		};
		attr('position', 3, 0);
		attr('normal', 3, 3);
		attr('isFace', 1, 6);
		const u = (name: string) => gl.getUniformLocation(prog, name);
		const uProjection = u('projection');
		const uCamZ = u('camZ');
		const uAngle = u('angle');
		const uFace = u('faceColor');
		const uSide = u('sideColor');
		gl.uniform3fv(u('sunDir'), SUN_DIR);
		gl.enable(gl.DEPTH_TEST);
		gl.enable(gl.CULL_FACE);
		gl.clearColor(0, 0, 0, 0);

		// Petit angle, caméra loin : un peu de perspective, sans que la
		// lettre la plus proche ne déborde du bouton en tournant.
		const fit = () => {
			const dpr = window.devicePixelRatio || 1;
			const cw = el.clientWidth || 1;
			const ch = el.clientHeight || 1;
			el.width = Math.round(cw * dpr);
			el.height = Math.round(ch * dpr);
			gl.viewport(0, 0, el.width, el.height);
			const aspect = cw / ch;
			const tan = Math.tan(((FOV / 2) * Math.PI) / 180);
			// Le texte occupe 62 % de la largeur du bouton.
			gl.uniform1f(uCamZ, width / 0.62 / (2 * tan * aspect) + DEPTH);
			const near = 0.1;
			const far = 1000;
			const f = 1 / tan;
			// prettier-ignore
			gl.uniformMatrix4fv(uProjection, false, [
				f / aspect, 0, 0, 0,
				0, f, 0, 0,
				0, 0, (far + near) / (near - far), -1,
				0, 0, (2 * far * near) / (near - far), 0
			]);
		};
		fit();
		const ro = new ResizeObserver(() => {
			fit();
			draw(performance.now());
		});
		ro.observe(el);

		const still = angle !== null || matchMedia('(prefers-reduced-motion: reduce)').matches;
		const t0 = performance.now();
		let raf = 0;
		let last = -Infinity;
		let lost = false;
		const draw = (now: number) => {
			if (lost) return;
			const a = still ? ((angle ?? 20) * Math.PI) / 180 : ((now - t0) / TURN_MS) * Math.PI * 2;
			gl.uniform1f(uAngle, a);
			gl.uniform3fv(uFace, linear(COLORS.face[colorKey]));
			gl.uniform3fv(uSide, linear(COLORS.side[colorKey]));
			gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
			gl.drawArrays(gl.TRIANGLES, 0, count);
			last = now;
			ready = true;
		};
		const frame = (now: number) => {
			if (now - last >= MIN_FRAME_MS) draw(now);
			raf = requestAnimationFrame(frame);
		};
		if (still) {
			draw(performance.now());
			redraw = () => draw(performance.now());
		} else {
			raf = requestAnimationFrame(frame);
		}

		// Contexte perdu (pilote réinitialisé) : le texte à plat revient.
		const onLost = (e: Event) => {
			e.preventDefault();
			lost = true;
			ready = false;
			cancelAnimationFrame(raf);
		};
		el.addEventListener('webglcontextlost', onLost);

		return () => {
			cancelAnimationFrame(raf);
			ro.disconnect();
			el.removeEventListener('webglcontextlost', onLost);
			redraw = null;
			gl.deleteBuffer(buf);
			gl.deleteProgram(prog);
			gl.getExtension('WEBGL_lose_context')?.loseContext();
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
