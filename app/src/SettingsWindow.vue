<script setup>
import { computed, defineAsyncComponent, onMounted, onUnmounted, ref } from 'vue'
import { Pin, RefreshCw, SlidersHorizontal } from '@lucide/vue'
import { usePetSettings } from './composables/usePetSettings.js'
import { onExpressionObserved, reloadPetModel, requestExpression } from './api/pet.js'

const shared = usePetSettings()
const { ready, settings, settingsBusy, error, setMaxFps, setAlwaysOnTop } = shared
const isDev = import.meta.env.DEV
const DevPanel = isDev ? defineAsyncComponent(() => import('./components/DevPanel.vue')) : null
const panelOpen = ref(false)
const sending = ref(false)
const lastExpression = ref('')
const receivedCount = ref(0)
const requestStatus = ref('尚未发送')
const observedReady = ref(false)
let disposed = false
let unlisten
onMounted(async () => {
  if (!isDev) return
  try {
    const stop = await onExpressionObserved(name => {
      if (disposed) return
      lastExpression.value = name
      receivedCount.value++
    })
    if (disposed) { stop(); return }
    unlisten = stop
    observedReady.value = true
  } catch (cause) { if (!disposed) error.value = String(cause) }
})
onUnmounted(() => { disposed = true; unlisten?.() })
async function sendExpression(name) {
  if (sending.value) return
  sending.value = true
  error.value = ''
  try {
    await requestExpression(name)
    requestStatus.value = `请求已发送：${name}`
  } catch (cause) { requestStatus.value = `请求失败：${name}`; error.value = String(cause) }
  finally { sending.value = false }
}
async function reload() {
  try { await reloadPetModel() }
  catch (cause) { error.value = String(cause) }
}
const debugPet = { ...shared, ready: computed(() => ready.value && observedReady.value), sending, lastExpression, receivedCount, requestStatus, sendExpression }
</script>

<template>
  <main class="settings-page">
    <header class="settings-heading"><span class="settings-mark"><SlidersHorizontal :size="21" :stroke-width="1.6" aria-hidden="true" /></span><div><h1>陪伴，随你心意</h1><p>八千代的日常设置</p></div></header>
    <section class="settings-card" aria-label="桌宠设置" :aria-busy="!ready || settingsBusy">
      <div class="setting-row">
        <div><label for="pet-fps">动画帧率</label><p>流畅陪伴，或节省一点电量</p></div>
        <select id="pet-fps" :value="settings?.maxFps ?? 30" :disabled="!ready || settingsBusy" @change="setMaxFps(Number($event.target.value))"><option :value="30">30 FPS</option><option :value="15">15 FPS</option></select>
      </div>
      <div class="setting-row">
        <div><label id="pin-label" for="pet-pin">保持置顶</label><p>让八千代留在其他窗口上方</p></div>
        <button id="pet-pin" class="toggle" role="switch" :aria-checked="settings?.alwaysOnTop ?? false" aria-labelledby="pin-label" :disabled="!ready || settingsBusy" @click="setAlwaysOnTop(!settings?.alwaysOnTop)"><span /></button>
      </div>
    </section>
    <p class="settings-note"><Pin :size="13" aria-hidden="true" />设置即时生效，也可从托盘找回八千代。</p>
    <p v-if="error" class="error" role="alert">{{ error }}</p>
    <footer v-if="isDev" class="settings-dev"><button class="text-button" :aria-expanded="panelOpen" @click="panelOpen = !panelOpen">{{ panelOpen ? '收起联调' : '开发联调' }}</button><button class="text-button" @click="reload"><RefreshCw :size="13" aria-hidden="true" />重新加载模型</button></footer>
    <DevPanel v-if="isDev && panelOpen" :pet="debugPet" />
  </main>
</template>
