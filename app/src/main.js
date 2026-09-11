import { createApp } from 'vue'
import './style.css'

async function mount() {
  const settingsView = new URLSearchParams(location.search).get('view') === 'settings'
  if (settingsView) {
    document.body.classList.add('settings-window')
    document.title = '八千代 · 设置'
    const { default: SettingsWindow } = await import('./SettingsWindow.vue')
    createApp(SettingsWindow).mount('#app')
    return
  }
  // Core 和模型依赖只进入主窗口，独立设置页不创建第二份渲染器。
  await new Promise((resolve, reject) => {
    const script = document.createElement('script')
    script.src = '/live2d/live2dcubismcore.min.js'
    script.onload = resolve
    script.onerror = () => reject(new Error('无法加载 Live2D Core，请重启桌宠后重试。'))
    document.head.append(script)
  })
  const { default: App } = await import('./App.vue')
  createApp(App).mount('#app')
}
mount().catch(cause => {
  const message = document.createElement('p')
  message.className = 'error boot-error'
  message.role = 'alert'
  message.textContent = String(cause)
  document.getElementById('app').replaceChildren(message)
})
