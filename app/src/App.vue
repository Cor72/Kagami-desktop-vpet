<script setup>
import { ref } from 'vue'

// 第一阶段：计数只存在于 Vue 中，关闭窗口后会重置。
const count = ref(0)
</script>

<template>
  <main>
    <h1>八千代桌宠</h1>
    <button type="button" @click="count++">
      <span aria-live="polite">点击了 {{ count }} 次</span>
    </button>
  </main>
</template>
