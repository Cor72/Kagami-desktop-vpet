import { onMounted, onUnmounted, ref, shallowRef } from 'vue'
import { getPetSettings, onPetSettingsChanged, setPetAlwaysOnTop, setPetMaxFps, setPetVisible } from '../api/pet.js'
import { latestSettings } from './petSettings.js'

// 两个窗口共享 Rust 状态；订阅先于初始读取，版本号避免旧快照覆盖新事件。
export function usePetSettings() {
  const ready = ref(false)
  const settings = shallowRef(null)
  const settingsBusy = ref(false)
  const error = ref('')
  let disposed = false
  let unlisten
  const accept = snapshot => {
    if (!disposed) settings.value = latestSettings(settings.value, snapshot)
  }
  onMounted(async () => {
    try {
      const stop = await onPetSettingsChanged(accept)
      if (disposed) { stop(); return }
      unlisten = stop
      accept(await getPetSettings())
      if (!disposed) ready.value = true
    } catch (cause) {
      if (!disposed) error.value = `读取桌宠设置失败：${String(cause)}`
    }
  })
  onUnmounted(() => { disposed = true; unlisten?.() })

  async function change(command) {
    if (!ready.value || settingsBusy.value) return false
    settingsBusy.value = true
    error.value = ''
    try { accept(await command()); return true }
    catch (cause) { if (!disposed) error.value = String(cause); return false }
    finally { if (!disposed) settingsBusy.value = false }
  }
  return {
    ready, settings, settingsBusy, error,
    setVisible: visible => change(() => setPetVisible(visible)),
    setMaxFps: fps => change(() => setPetMaxFps(fps)),
    setAlwaysOnTop: enabled => change(() => setPetAlwaysOnTop(enabled)),
  }
}
