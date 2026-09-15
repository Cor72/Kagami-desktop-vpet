// AI 设置的共享状态：设置窗口与对话窗口都用它读同一份 Rust 状态。
//
// 与其他 composable 同一套约定：先订阅事件再读初始值，版本号防止旧快照覆盖新事件。

import { onMounted, onUnmounted, ref, shallowRef } from 'vue'
import {
  clearApiKey, getAiConfig, onAiConfigChanged, setAiConfig, setApiKey, testAiConnection,
} from '../api/chat.js'
import { latestAiConfig } from './aiConfig.js'

export function useAiConfig() {
  const ready = ref(false)
  const config = shallowRef(null)
  const busy = ref(false)
  const testing = ref(false)
  const error = ref('')
  const testResult = shallowRef(null)

  let disposed = false
  let unlisten

  const accept = snapshot => {
    if (!disposed) config.value = latestAiConfig(config.value, snapshot)
  }

  onMounted(async () => {
    try {
      const stop = await onAiConfigChanged(accept)
      if (disposed) {
        stop()
        return
      }
      unlisten = stop
      accept(await getAiConfig())
      if (!disposed) ready.value = true
    } catch (cause) {
      if (!disposed) error.value = `读取 AI 设置失败：${String(cause)}`
    }
  })

  onUnmounted(() => {
    disposed = true
    unlisten?.()
  })

  /** 跑一个会改配置的命令：命令返回新快照，直接拿它更新界面。 */
  async function run(command) {
    if (busy.value) return false
    busy.value = true
    error.value = ''
    try {
      accept(await command())
      return true
    } catch (cause) {
      if (!disposed) error.value = String(cause)
      return false
    } finally {
      if (!disposed) busy.value = false
    }
  }

  return {
    ready,
    config,
    busy,
    testing,
    error,
    testResult,
    setProvider: provider => run(() => setAiConfig({ provider })),
    setModel: model => run(() => setAiConfig({ model })),
    setBaseUrl: baseUrl => run(() => setAiConfig({ baseUrl })),
    setMode: mode => run(() => setAiConfig({ mode })),
    // Key 只在保存的那一刻经过内存，命令不返回它，这里也不留副本。
    saveKey: key => {
      testResult.value = null
      return run(() => setApiKey(config.value.provider, key))
    },
    clearKey: () => {
      testResult.value = null
      return run(() => clearApiKey(config.value.provider))
    },
    test: async () => {
      if (testing.value) return
      testing.value = true
      error.value = ''
      try {
        testResult.value = await testAiConnection()
      } catch (cause) {
        testResult.value = { ok: false, message: String(cause) }
      } finally {
        if (!disposed) testing.value = false
      }
    },
  }
}
