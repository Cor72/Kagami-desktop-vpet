import { createApp } from 'vue'
import './style.css'

// 窗口分流：主窗口 / 设置 / 对话各自挂载一个根组件，互不共用渲染器。
async function mountSettings() {
  document.body.classList.add('settings-window')
  document.title = '八千代 · 设置'
  const { default: SettingsWindow } = await import('./SettingsWindow.vue')
  createApp(SettingsWindow).mount('#app')
}

// 对话窗口同样不加载 Live2D Core：那里没有画布，也不需要第二份渲染器。
async function mountChat() {
  document.body.classList.add('chat-window')
  document.title = '八千代 · 对话'
  const { default: ChatWindow } = await import('./ChatWindow.vue')
  createApp(ChatWindow).mount('#app')
}

async function mountPet() {
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

const view = new URLSearchParams(location.search).get('view')
const mount = view === 'settings' ? mountSettings : view === 'chat' ? mountChat : mountPet

mount().catch(cause => {
  const message = document.createElement('p')
  message.className = 'error boot-error'
  message.role = 'alert'
  message.textContent = String(cause)
  document.getElementById('app').replaceChildren(message)
})
