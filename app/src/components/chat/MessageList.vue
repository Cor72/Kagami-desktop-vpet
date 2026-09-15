<script setup>
// 消息流：可滚动，新内容到达时贴到底部——但用户自己往上翻看历史时就不打扰他。
import { nextTick, onMounted, ref, watch } from 'vue'
import MessageBubble from './MessageBubble.vue'

const props = defineProps({
  messages: { type: Array, required: true },
})

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
    <MessageBubble v-for="message in messages" :key="message.id" :message="message" />
  </div>
</template>
