<script setup>
// 消息流：可滚动，新内容到达时贴到底部——但用户自己往上翻看历史时就不打扰他。
//
// 一条助手消息下面可能挂着若干张工具卡片（「正在读取 xxx」）与一张写入确认卡片。
// 它们都跟着 messageId 走，所以关掉窗口再打开时不会留下孤儿卡片。
import { nextTick, onMounted, ref, watch } from 'vue'
import MessageBubble from './MessageBubble.vue'
import ToolCallCard from './ToolCallCard.vue'
import WriteConfirmCard from './WriteConfirmCard.vue'

const props = defineProps({
  messages: { type: Array, required: true },
  /** 用户按下「应用」/「拒绝」之后到 Rust 回话之前，按钮先禁掉，避免连点两次。 */
  writeBusy: Boolean,
})

const emit = defineEmits(['apply-write', 'reject-write'])

const scroller = ref(null)
const atBottom = ref(true)

function updateAtBottom() {
  const element = scroller.value
  if (!element) return
  atBottom.value = element.scrollHeight - element.scrollTop - element.clientHeight < 60
}

async function scrollToBottom() {
  await nextTick()
  const element = scroller.value
  if (element) element.scrollTop = element.scrollHeight
}

watch(
  () => props.messages,
  () => {
    if (atBottom.value) void scrollToBottom()
  },
  { deep: true },
)

onMounted(scrollToBottom)
</script>

<template>
  <div ref="scroller" class="chat-log" aria-label="对话消息" aria-live="polite" @scroll.passive="updateAtBottom">
    <p v-if="!messages.length" class="chat-empty">打个招呼吧，八千代在听。</p>
    <template v-for="message in messages" :key="message.id">
      <MessageBubble :message="message" />
      <ToolCallCard v-for="tool in message.tools ?? []" :key="tool.callId" :tool="tool" />
      <WriteConfirmCard
        v-if="message.writeRequest"
        :request="message.writeRequest"
        :busy="writeBusy"
        @apply="emit('apply-write', $event)"
        @reject="emit('reject-write', $event)"
      />
    </template>
  </div>
</template>
