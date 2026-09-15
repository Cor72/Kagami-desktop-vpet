<script setup>
// 对话窗口：顶栏（标题 + 模式切换 + 设置入口）、消息流、输入区。
// 窗口本体由 Rust 的 chat_window.rs 创建；这里只负责渲染与交互。
import { computed, ref } from 'vue'
import { MessageCircle, Send, Settings as SettingsIcon, Square } from '@lucide/vue'
import { AGENT_MODE, CHAT_MODE } from './api/chat.js'
import { openPetSettings } from './api/pet.js'

const modes = [
  { id: CHAT_MODE, label: '聊天' },
  { id: AGENT_MODE, label: 'Agent' },
]
const mode = ref(CHAT_MODE)
const draft = ref('')
const notice = ref('')

const isAgent = computed(() => mode.value === AGENT_MODE)
const modeHint = computed(() =>
  isAgent.value ? 'Agent 模式：八千代可以读你授权的工作区' : '聊天模式：只聊天，不接触任何文件')

function selectMode(id) {
  if (mode.value === id) return
  mode.value = id
  notice.value = ''
}

async function openSettings() {
  try {
    await openPetSettings()
  } catch (cause) {
    notice.value = String(cause)
  }
}

function submit() {
  // 消息收发要等模型服务接上之后才有意义；先如实说明，不做假的假象。
  notice.value = '模型服务还没接入：填好 API Key 之后这句话才发得出去。'
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
      <button type="button" class="chat-icon-button" data-tooltip="设置" aria-label="打开设置" @click="openSettings">
        <SettingsIcon :size="17" :stroke-width="1.7" aria-hidden="true" />
      </button>
    </header>

    <p class="chat-mode-hint">{{ modeHint }}</p>

    <section class="chat-log" aria-label="对话消息" aria-live="polite">
      <p class="chat-empty">还没有消息。</p>
    </section>

    <p v-if="notice" class="chat-notice" role="status">{{ notice }}</p>

    <footer class="chat-composer">
      <input v-model="draft" type="text" class="chat-input" placeholder="说点什么…" aria-label="输入消息" @keydown.enter.prevent="submit" />
      <button type="button" class="chat-send" :disabled="!draft.trim()" @click="submit"><Send :size="15" aria-hidden="true" />发送</button>
      <button type="button" class="chat-stop" disabled><Square :size="14" aria-hidden="true" />停止</button>
    </footer>
  </main>
</template>
