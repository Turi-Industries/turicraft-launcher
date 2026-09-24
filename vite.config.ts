import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

// Tauri charge l'interface depuis des fichiers statiques : adapter-static,
// une seule page (fallback), pas de rendu côté serveur.
export default defineConfig(({ command }) => ({
	plugins: [
		sveltekit({
			// En développement, le style de chaque composant voyage avec son code
			// au lieu d'un module CSS séparé : après des rechargements à chaud,
			// Vite perdait ces modules (« failed to load virtual css module ») et
			// des écrans restaient sans marges ni défilement (24/09).
			vitePlugin: { emitCss: command === 'build' },
			compilerOptions: {
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true
			},
			adapter: adapter({ fallback: 'index.html' })
		})
	],
	// Port fixe attendu par tauri.conf.json (devUrl).
	clearScreen: false,
	server: { port: 1420, strictPort: true, watch: { ignored: ['**/src-tauri/**'] } }
}));
