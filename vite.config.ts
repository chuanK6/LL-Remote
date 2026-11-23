import { defineConfig } from 'vitest/config'
import vue from '@vitejs/plugin-vue'

export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: {
    host: '127.0.0.1', port: 1420, strictPort: true,
    watch: { ignored: ['**/src-tauri/target/**', '**/.tools/**', '**/.work/**', '**/third-party/**', '**/engines/**', '**/runtimes/**', '**/release/**'] },
    fs: { deny: ['**/.git/**', '**/credentials/**', '**/Data/**', '**/.work/**', '**/.tools/**', '**/src-tauri/target/**'] },
  },
  build: { target: 'es2022', sourcemap: false },
  test: { environment: 'jsdom', include: ['tests/unit/**/*.test.ts'], restoreMocks: true },
})
