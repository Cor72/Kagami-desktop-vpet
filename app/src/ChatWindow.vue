<script setup>
// 对话窗口：顶栏（标题 + 模式切换 + 新建对话 + 设置入口）、工作区栏（仅 Agent 模式）、
// 消息流（含工具卡片与写入确认）、输入区。
// 窗口本体由 Rust 的 chat_window.rs 创建；这里只负责渲染与交互。
import { computed, ref, watch } from 'vue'
import { MessageCircle, Plus, Settings as SettingsIcon } from '@lucide/vue'
import MessageList from './components/chat/MessageList.vue'
import ChatComposer from './components/chat/ChatComposer.vue'
import WorkspaceBar from './components/chat/WorkspaceBar.vue'
import { useChat } from './composables/useChat.js'
import { useWorkspace } from './composables/useWorkspace.js'
import { useAiConfig } from './composables/useAiConfig.js'
import { AGENT_MODE, CHAT_MODE, providerPreset } from './composables/aiConfig.js'
import { openPetSettings } from './api/pet.js'

const modes = [
  { id: CHAT_MODE, label: '聊天' },
  { id: AGENT_MODE, label: 'Agent' },
]
const draft = ref('')
/** 用户按下「应用」/「拒绝」之后到 Rust 回话之前，把按钮禁掉，免得连点两次。 */
const writeBusy = ref(false)

const {
  ready: chatReady, busy, streamingId, messages, error, notice, send, stop, startNewSession,
  applyWrite, rejectWrite,
} = useChat()
const {
  ready: aiReady, config: aiConfig, error: aiError, setMode,
} = useAiConfig()

const mode = computed(() => aiConfig.value?.mode ?? CHAT_MODE)
const isAgent = computed(() => mode.value === AGENT_MODE)
// 拖拽只在 Agent 模式生效：聊天模式下拖进来一个文件就悄悄授权，用户会莫名其妙。
const {
  entries, busy: workspaceBusy, dragging, error: workspaceError,
  addDir, remove: removeEntry,
} = useWorkspace({ active: () => isAgent.value })
const provider = computed(() => providerPreset(aiConfig.value?.provider))
const hasKey = computed(() => aiConfig.value?.hasKey === true)
// 提示行同时告诉用户「在用哪家的哪个模型」——这是最容易配错的地方。
const modeHint = computed(() => {
  const model = aiConfig.value?.model || provider.value.model || '未设置模型'
  const tail = `${provider.value.label} · ${model}`
  return isAgent.value
    ? `Agent 模式 · ${tail} ｜ 只碰你给的那几样，改文件前先给你看 diff`
    : `聊天模式 · ${tail}`
})

// 切回聊天模式时把拖拽高亮清掉，免得留着一块「松手就加进来」的提示。
watch(isAgent, value => {
  if (!value) dragging.value = false
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

/** 「应用」：Rust 侧会再校验一次路径，然后才写盘。 */
async function onApplyWrite(requestId) {
  if (writeBusy.value) return
  writeBusy.value = true
  try {
    await applyWrite(requestId)
  } finally {
    writeBusy.value = false
  }
}

async function onRejectWrite(requestId) {
  if (writeBusy.value) return
  writeBusy.value = true
  try {
    await rejectWrite(requestId)
  } finally {
    writeBusy.value = false
  }
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

    <WorkspaceBar
      v-if="isAgent"
      :entries="entries"
      :busy="workspaceBusy"
      :dragging="dragging"
      :error="workspaceError"
      @add-dir="addDir"
      @remove="removeEntry"
    />

    <MessageList
      :messages="messages"
      :write-busy="writeBusy"
      @apply-write="onApplyWrite"
      @reject-write="onRejectWrite"
    />

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
