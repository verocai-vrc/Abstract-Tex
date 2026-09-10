import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

export default {
  // Lets <script lang="ts"> work inside .svelte files.
  preprocess: vitePreprocess(),
  compilerOptions: {
    // Svelte 5 runes everywhere; no legacy component syntax in this codebase.
    runes: true,
  },
};
