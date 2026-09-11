import { computed, onMounted, onUnmounted, ref, shallowRef } from 'vue'
import {
  onExpressionRequested, onPetDesktopError,
  quitPet, requestExpression,
  isPetMinimized, onPetMinimized,
} from '../api/pet.js'
import { usePetSettings } from './usePetSettings.js'
import { selectRenderPolicy } from '../live2d/renderPolicy.js'

export function usePet() {
  const shared = usePetSettings()
  const { settings, settingsBusy, error } = shared
  const ready = ref(false)
  const sending = ref(false)
  const requestStatus = ref('尚未发送')
  const lastExpression = ref('')
  const receivedCount = ref(0)
  const expressionRequest = shallowRef(null)
  const pageVisible = ref(document.visibilityState === 'visible')
  const minimized = ref(false)
  const renderPolicy = computed(() => selectRenderPolicy(
    settings.value?.visible ?? false, pageVisible.value, settings.value?.maxFps ?? 30, minimized.value,
  ))

  let disposed = false
  const unlisteners = []
  let minimizeEventVersion = 0

  function updatePageVisibility() {
    pageVisible.value = document.visibilityState === 'visible'
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
    document.addEventListener('visibilitychange', updatePageVisibility)
    updatePageVisibility()
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
        () => onPetDesktopError(message => { if (!disposed) error.value = message }),
        () => onPetMinimized(value => {
          if (disposed) return
          minimizeEventVersion++
          minimized.value = value
        }),
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
      const version = minimizeEventVersion
      const initialMinimized = await isPetMinimized()
      if (!disposed && version === minimizeEventVersion) minimized.value = initialMinimized
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
    document.removeEventListener('visibilitychange', updatePageVisibility)
    for (const unlisten of unlisteners) void stopListener(unlisten)
  })

  async function sendExpression(name) {
    if (!ready.value || sending.value) return false

    sending.value = true
    error.value = ''
    requestStatus.value = `正在发送：${name}`

    try {
      await requestExpression(name)
      if (!disposed) requestStatus.value = `请求已发送：${name}（Rust 命令成功返回）`
      return true
    } catch (cause) {
      if (!disposed) {
        requestStatus.value = `请求失败：${name}`
        error.value = String(cause)
      }
      return false
    } finally {
      if (!disposed) sending.value = false
    }
  }

  async function quit() {
    try { await quitPet() }
    catch (cause) { if (!disposed) error.value = String(cause) }
  }

  return {
    ...shared,
    ready, sending, requestStatus, lastExpression, receivedCount, expressionRequest, error, sendExpression,
    settings, settingsBusy, renderPolicy, quit,
  }
}
