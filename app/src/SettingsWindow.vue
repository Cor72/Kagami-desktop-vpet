<script setup>
import { computed, defineAsyncComponent, onMounted, onUnmounted, ref } from 'vue'
import { KeyRound, Pin, RefreshCw, SlidersHorizontal } from '@lucide/vue'
import { usePetSettings } from './composables/usePetSettings.js'
import { useAiConfig } from './composables/useAiConfig.js'
import { PROVIDERS, isCustomProvider } from './composables/aiConfig.js'
import { onExpressionObserved, reloadPetModel, requestExpression } from './api/pet.js'

const shared = usePetSettings()
const { ready, settings, settingsBusy, error, setMaxFps, setAlwaysOnTop } = shared
// AI 一节：与对话窗口共享同一份 Rust 状态，谁改了另一边都会收到 ai-config-changed。
const {
  ready: aiReady, config: aiConfig, busy: aiBusy, testing: aiTesting, error: aiError,
  testResult, setProvider, setModel, setBaseUrl, saveKey, clearKey, test: testConnection,
} = useAiConfig()
const keyDraft = ref('')
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

const isCustom = computed(() => isCustomProvider(aiConfig.value?.provider))
const providerLabel = computed(
  () => PROVIDERS.find(provider => provider.id === aiConfig.value?.provider)?.label ?? '服务商',
)

async function submitKey() {
  const value = keyDraft.value.trim()
  if (!value || aiBusy.value) return
  // 保存成功后立刻清空输入框：Key 不留在这个组件里。
  if (await saveKey(value)) keyDraft.value = ''
}

async function removeKey() {
  await clearKey()
}
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
    <section class="settings-card ai-card" aria-label="AI 设置" :aria-busy="!aiReady || aiBusy">
      <h2 class="settings-section-title"><KeyRound :size="14" aria-hidden="true" />AI</h2>
      <div class="setting-row">
        <div><label for="ai-provider">服务商</label><p>默认 DeepSeek，三家都是 OpenAI 兼容格式</p></div>
        <select id="ai-provider" :value="aiConfig?.provider ?? 'deepseek'" :disabled="!aiReady || aiBusy" @change="setProvider($event.target.value)">
          <option v-for="provider in PROVIDERS" :key="provider.id" :value="provider.id">{{ provider.label }}</option>
        </select>
      </div>
      <div class="setting-row stacked">
        <div><label for="ai-key">API Key</label><p>存在 Windows 凭据管理器里，不写进配置文件、不回传界面</p></div>
        <div class="ai-key-row">
          <input
            id="ai-key"
            v-model="keyDraft"
            class="ai-input"
            type="password"
            autocomplete="off"
            spellcheck="false"
            :placeholder="aiConfig?.hasKey ? '粘贴新 Key 可覆盖' : 'sk-…'"
            :disabled="!aiReady || aiBusy"
            @keydown.enter.prevent="submitKey"
          />
          <button type="button" class="text-button" :disabled="!keyDraft.trim() || aiBusy" @click="submitKey">保存</button>
        </div>
        <p v-if="aiConfig?.hasKey" class="ai-mask">
          已保存：{{ aiConfig.keyMask }}
          <button type="button" class="text-button danger" :disabled="aiBusy" @click="removeKey">删除</button>
        </p>
      </div>
      <div v-if="isCustom" class="setting-row stacked">
        <div><label for="ai-base-url">Base URL</label><p>填到 /v1 为止；粘贴完整地址也会自动截好</p></div>
        <input
          id="ai-base-url"
          class="ai-input"
          type="text"
          spellcheck="false"
          placeholder="https://example.com/v1"
          :value="aiConfig?.baseUrl ?? ''"
          :disabled="!aiReady || aiBusy"
          @change="setBaseUrl($event.target.value)"
        />
      </div>
      <div class="setting-row stacked">
        <div><label for="ai-model">模型名</label><p>换了服务商会自动填默认模型</p></div>
        <input
          id="ai-model"
          class="ai-input"
          type="text"
          spellcheck="false"
          :value="aiConfig?.model ?? ''"
          :disabled="!aiReady || aiBusy"
          @change="setModel($event.target.value)"
        />
      </div>
      <div class="setting-row">
        <div><label>连接</label><p>发一次最小请求，确认真能连上</p></div>
        <button type="button" class="text-button" :disabled="!aiReady || aiBusy" @click="testConnection">
          {{ aiTesting ? '测试中…' : '测试连接' }}
        </button>
      </div>
      <p v-if="aiTesting" class="ai-status">正在联系 {{ providerLabel }}…</p>
      <p v-else-if="testResult" class="ai-status" :class="testResult.ok ? 'ok' : 'fail'" role="status">{{ testResult.message }}</p>
      <p v-if="aiError" class="error" role="alert">{{ aiError }}</p>
    </section>
    <footer v-if="isDev" class="settings-dev"><button class="text-button" :aria-expanded="panelOpen" @click="panelOpen = !panelOpen">{{ panelOpen ? '收起联调' : '开发联调' }}</button><button class="text-button" @click="reload"><RefreshCw :size="13" aria-hidden="true" />重新加载模型</button></footer>
    <DevPanel v-if="isDev && panelOpen" :pet="debugPet" />
  </main>
</template>
