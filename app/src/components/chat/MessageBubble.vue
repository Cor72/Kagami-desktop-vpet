<script setup>
// 单条消息：用户靠右、八千代靠左。正在出字时结尾挂一个光标块。
import { computed } from 'vue'

const props = defineProps({
  message: { type: Object, required: true },
})

const fromUser = computed(() => props.message.role === 'user')
</script>

<template>
  <article class="bubble-row" :class="fromUser ? 'from-user' : 'from-pet'">
    <div class="bubble" :class="fromUser ? 'bubble-user' : 'bubble-pet'">
      <p class="bubble-text">
        {{ message.content }}<span v-if="message.streaming" class="bubble-caret" aria-hidden="true" />
      </p>
      <p v-if="message.error" class="bubble-note">{{ message.error }}</p>
    </div>
  </article>
</template>
