<script setup>
// 工具调用卡片：「正在读取 pomodoro.rs」→「读过 pomodoro.rs 了」。
//
// 文案（`label` / `summary`）来自 Rust 的 `ai/tools.rs`，按角色设定写好——
// 她说的是「正在读取」，不是「正在执行 read_file 工具」。这里只负责显示与状态样式。
import { computed } from 'vue'
import { Check, Loader, TriangleAlert } from '@lucide/vue'

const props = defineProps({
  tool: { type: Object, required: true },
})

const running = computed(() => props.tool.status === 'running')
const failed = computed(() => props.tool.status === 'failed')
const text = computed(() => (running.value ? props.tool.label : props.tool.summary || props.tool.label))
</script>

<template>
  <div class="tool-row">
    <div class="tool-card" :class="{ running, failed }">
      <span class="tool-icon" aria-hidden="true">
        <Loader v-if="running" :size="13" class="tool-spin" />
        <TriangleAlert v-else-if="failed" :size="13" />
        <Check v-else :size="13" />
      </span>
      <span class="tool-text">{{ text }}</span>
    </div>
  </div>
</template>
