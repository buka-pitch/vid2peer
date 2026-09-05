import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  server: {
    host: true,
    port: 5173,
    allowedHosts: ['.monkeycode-ai.live'],
    proxy: {
      '/signal': {
        target: 'ws://127.0.0.1:9091',
        ws: true,
        changeOrigin: true,
      },
    },
  },
  preview: {
    host: true,
    port: 5173,
    allowedHosts: ['.monkeycode-ai.live'],
    proxy: {
      '/signal': {
        target: 'ws://127.0.0.1:9091',
        ws: true,
        changeOrigin: true,
      },
    },
  },
})
