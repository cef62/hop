import { unified } from '@astrojs/markdown-remark';
import { defineConfig } from 'astro/config';
import rehypeRepoLinks from './plugins/rehype-repo-links.mjs';
import remarkCommitLinks from './plugins/remark-commit-links.mjs';
import remarkDropPreamble from './plugins/remark-drop-preamble.mjs';

export default defineConfig({
  site: 'https://cef62.github.io',
  base: '/hop',
  markdown: {
    processor: unified({
      remarkPlugins: [remarkDropPreamble, remarkCommitLinks],
      rehypePlugins: [rehypeRepoLinks],
    }),
  },
  vite: {
    // Pages import README.md / CHANGELOG.md / the app icon from the repo root, outside
    // site/ (Vite's project root): allow serving them in dev.
    server: { fs: { allow: ['..'] } },
  },
});
