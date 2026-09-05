// Starts the signaling hub and the Vite dev server together so a single
// preview port can serve the UI and proxy /signal to the hub.

import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

const root = dirname(fileURLToPath(import.meta.url))
const frontend = join(root, '..')

const hub = spawn(process.execPath, [join(root, 'signal-hub.mjs')], {
  cwd: frontend,
  stdio: 'inherit',
  env: process.env,
})

const vite = spawn('npx', ['vite'], {
  cwd: frontend,
  stdio: 'inherit',
  env: process.env,
})

function shutdown() {
  hub.kill('SIGTERM')
  vite.kill('SIGTERM')
}

process.on('SIGINT', shutdown)
process.on('SIGTERM', shutdown)

hub.on('exit', (code) => {
  if (code && code !== 0) process.exit(code)
})
vite.on('exit', (code) => {
  hub.kill('SIGTERM')
  process.exit(code ?? 0)
})
