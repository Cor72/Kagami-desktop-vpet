import { onMounted, onUnmounted, ref, shallowRef } from 'vue'
import { onExpressionRequested, requestExpression } from '../api/pet.js'

export function usePet() {
  const ready = ref(false)
  const sending = ref(false)
  const requestStatus = ref('尚未发送')
  const lastExpression = ref('')
  const receivedCount = ref(0)
  const expressionRequest = shallowRef(null)
  const error = ref('')

  let disposed = false
  let unlisten

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
      const stopListening = await onExpressionRequested(name => {
        if (disposed) return

        // 收到事件才更新这里；invoke 成功不会修改事件记录。
        lastExpression.value = name
        receivedCount.value++
        // 每次事件都创建新对象，同一个表情连续点击也会通知模型。
        expressionRequest.value = { name, sequence: receivedCount.value }
        console.log('[Vue] 收到 Rust 事件:', name)
      })

      // 订阅是异步的，组件可能在订阅完成前就已被卸载。
      if (disposed) {
        await stopListener(stopListening)
        return
      }

      unlisten = stopListening
      ready.value = true
    } catch (cause) {
      if (!disposed) {
        error.value = `事件订阅失败：${String(cause)}。请通过 pnpm tauri dev 启动桌面应用。`
      }
    }
  })

  onUnmounted(() => {
    disposed = true
    ready.value = false
    void stopListener(unlisten)
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

  return { ready, sending, requestStatus, lastExpression, receivedCount, expressionRequest, error, sendExpression }
}
