import { onMounted, onUnmounted, ref, shallowRef } from 'vue'
import {
  getPetSettings, onExpressionRequested, onPetDesktopError, onPetSettingsChanged,
  quitPet, requestExpression, setPetAlwaysOnTop, setPetMaxFps, setPetVisible,
} from '../api/pet.js'
import { latestSettings } from './petSettings.js'

export function usePet() {
  const ready = ref(false)
  const sending = ref(false)
  const requestStatus = ref('尚未发送')
  const lastExpression = ref('')
  const receivedCount = ref(0)
  const expressionRequest = shallowRef(null)
  const error = ref('')
  const settings = shallowRef(null)
  const settingsBusy = ref(false)

  let disposed = false
  const unlisteners = []

  function acceptSettings(snapshot) {
    if (!disposed) settings.value = latestSettings(settings.value, snapshot)
  }

  async function stopListener(stop) {
    try {
      await stop?.()
    } catch (cause) {
      // 组件已经卸载，清理失败记录到 DevTools，避免未处理的 Promise。
      console.error('[Vue] 取消事件监听失败:', cause)
    }
  }

  onMounted(async () => {
    try {
      const subscriptions = [
        () => onExpressionRequested(name => {
          if (disposed) return

          // 收到事件才更新这里；invoke 成功不会修改事件记录。
          lastExpression.value = name
          receivedCount.value++
          // 每次事件都创建新对象，同一个表情连续点击也会通知模型。
          expressionRequest.value = { name, sequence: receivedCount.value }
          console.log('[Vue] 收到 Rust 事件:', name)
        }),
        () => onPetSettingsChanged(acceptSettings),
        () => onPetDesktopError(message => { if (!disposed) error.value = message }),
      ]
      for (const subscribe of subscriptions) {
        const stopListening = await subscribe()
        // 订阅是异步的，组件可能在订阅完成前就已被卸载。
        if (disposed) {
          await stopListener(stopListening)
          return
        }
        unlisteners.push(stopListening)
      }
      // 先监听，再读取；较旧的初始快照不能覆盖期间到达的新事件。
      acceptSettings(await getPetSettings())
      if (!disposed) ready.value = true
    } catch (cause) {
      if (!disposed) {
        error.value = `桌宠通信初始化失败：${String(cause)}。请通过 pnpm tauri dev 启动桌面应用。`
      }
    }
  })

  onUnmounted(() => {
    disposed = true
    ready.value = false
    for (const unlisten of unlisteners) void stopListener(unlisten)
  })

  async function sendExpression(name) {
    if (!ready.value || sending.value) return

    sending.value = true
    error.value = ''
    requestStatus.value = `正在发送：${name}`

    try {
      await requestExpression(name)
      if (!disposed) requestStatus.value = `请求已发送：${name}（Rust 命令成功返回）`
    } catch (cause) {
      if (!disposed) {
        requestStatus.value = `请求失败：${name}`
        error.value = String(cause)
      }
    } finally {
      if (!disposed) sending.value = false
    }
  }

  async function changeSettings(command) {
    if (!ready.value || settingsBusy.value) return
    settingsBusy.value = true
    error.value = ''
    try {
      acceptSettings(await command())
    } catch (cause) {
      if (!disposed) error.value = String(cause)
    } finally {
      if (!disposed) settingsBusy.value = false
    }
  }

  async function quit() {
    try { await quitPet() }
    catch (cause) { if (!disposed) error.value = String(cause) }
  }

  return {
    ready, sending, requestStatus, lastExpression, receivedCount, expressionRequest, error, sendExpression,
    settings, settingsBusy, quit,
    setVisible: visible => changeSettings(() => setPetVisible(visible)),
    setMaxFps: fps => changeSettings(() => setPetMaxFps(fps)),
    setAlwaysOnTop: enabled => changeSettings(() => setPetAlwaysOnTop(enabled)),
  }
}
