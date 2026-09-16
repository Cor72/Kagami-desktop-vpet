<script setup>
// 输入区：输入框 + 发送 + 停止。正在出字时发送禁用、停止可用。
import { Send, Square } from '@lucide/vue'

const props = defineProps({
  modelValue: { type: String, default: '' },
  busy: Boolean,
  streaming: Boolean,
  placeholder: { type: String, default: '说点什么…' },
})

const emit = defineEmits(['update:modelValue', 'submit', 'stop'])

function submit() {
  if (props.busy || !props.modelValue.trim()) return
  emit('submit')
}
</script>

<template>
  <footer class="chat-composer">
    <input
      class="chat-input"
      type="text"
      :value="modelValue"
      :placeholder="placeholder"
      aria-label="输入消息"
      @input="emit('update:modelValue', $event.target.value)"
      @keydown.enter.prevent="submit"
    />
    <button type="button" class="chat-send" :disabled="busy || !modelValue.trim()" @click="submit">
      <Send :size="15" aria-hidden="true" />发送
    </button>
    <button type="button" class="chat-stop" :disabled="!streaming" @click="emit('stop')">
      <Square :size="14" aria-hidden="true" />停止
    </button>
  </footer>
</template>
