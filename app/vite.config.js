import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: {
    host: '127.0.0.1',
    // Rust 文件交给 Tauri 监听，Vite 只负责前端热更新。
    watch: { ignored: ['**/src-tauri/**'] },
  },
})
