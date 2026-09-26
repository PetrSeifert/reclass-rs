import { svelte } from '@sveltejs/vite-plugin-svelte'
import { defineConfig } from 'vite'

// `npm run dev` proxies the API to a running reclass-server (default port 7878).
const server = process.env.RECLASS_SERVER ?? 'http://127.0.0.1:7878'

export default defineConfig({
  plugins: [svelte()],
  server: {
    proxy: {
      '/api': server,
      '/ws': { target: server.replace(/^http/, 'ws'), ws: true },
    },
  },
})
