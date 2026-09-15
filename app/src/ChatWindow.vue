<script setup>
// 对话窗口：顶栏（标题 + 模式切换 + 新建对话 + 设置入口）、消息流、输入区。
// 窗口本体由 Rust 的 chat_window.rs 创建；这里只负责渲染与交互。
import { computed, ref } from 'vue'
import { MessageCircle, Plus, Settings as SettingsIcon } from '@lucide/vue'
import MessageList from './components/chat/MessageList.vue'
import ChatComposer from './components/chat/ChatComposer.vue'
import { useChat } from './composables/useChat.js'
import { useAiConfig } from './composables/useAiConfig.js'
import { AGENT_MODE, CHAT_MODE, providerPreset } from './composables/aiConfig.js'
import { openPetSettings } from './api/pet.js'

const modes = [
  { id: CHAT_MODE, label: '聊天' },
  { id: AGENT_MODE, label: 'Agent' },
]
const draft = ref('')

const {
  ready: chatReady, busy, streamingId, messages, error, notice, send, stop, startNewSession,
} = useChat()
const {
  ready: aiReady, config: aiConfig, error: aiError, setMode,
} = useAiConfig()

const mode = computed(() => aiConfig.value?.mode ?? CHAT_MODE)
const isAgent = computed(() => mode.value === AGENT_MODE)
const provider = computed(() => providerPreset(aiConfig.value?.provider))
const hasKey = computed(() => aiConfig.value?.hasKey === true)
// 提示行同时告诉用户「在用哪家的哪个模型」——这是最容易配错的地方。
const modeHint = computed(() => {
  const model = aiConfig.value?.model || provider.value.model || '未设置模型'
  const tail = `${provider.value.label} · ${model}`
  return isAgent.value
    ? `Agent 模式 · ${tail} ｜ 读文件的能力还没做（阶段 C），现在只能聊天`
    : `聊天模式 · ${tail}`
})

async function selectMode(id) {
  if (mode.value === id) return
  await setMode(id)
}

async function openSettings() {
  try {
    await openPetSettings()
  } catch (cause) {
    notice.value = String(cause)
  }
}

async function submit() {
  if (!chatReady.value || busy.value) return
  if (await send(draft.value)) draft.value = ''
}
</script>

<template>
  <main class="chat-page">
    <header class="chat-topbar">
      <div class="chat-title">
        <span class="chat-mark"><MessageCircle :size="18" :stroke-width="1.7" aria-hidden="true" /></span>
        <h1>八千代</h1>
      </div>
      <div class="chat-modes" role="group" aria-label="对话模式">
        <button
          v-for="item in modes"
          :key="item.id"
          type="button"
          class="chat-mode"
          :class="{ active: mode === item.id }"
          :aria-pressed="mode === item.id"
          @click="selectMode(item.id)"
        >{{ item.label }}</button>
      </div>
      <button type="button" class="chat-icon-button" data-tooltip="新建对话" aria-label="新建对话" @click="startNewSession">
        <Plus :size="17" :stroke-width="1.7" aria-hidden="true" />
      </button>
      <button type="button" class="chat-icon-button" data-tooltip="设置" aria-label="打开设置" @click="openSettings">
        <SettingsIcon :size="17" :stroke-width="1.7" aria-hidden="true" />
      </button>
    </header>

    <p class="chat-mode-hint">{{ modeHint }}</p>

    <MessageList :messages="messages" />

    <section v-if="aiReady && !hasKey" class="chat-guide">
      <p>还没有填 {{ provider.label }} 的 API Key，现在还聊不起来。</p>
      <button type="button" class="text-button" @click="openSettings">去设置里填 Key</button>
    </section>

    <p v-if="error || aiError" class="chat-notice error" role="alert">{{ error || aiError }}</p>
    <p v-else-if="notice" class="chat-notice" role="status">{{ notice }}</p>

    <ChatComposer
      v-model="draft"
      :busy="busy || !chatReady"
      :streaming="Boolean(streamingId)"
      placeholder="说点什么…（Enter 发送）"
      @submit="submit"
      @stop="stop"
    />
  </main>
</template>
