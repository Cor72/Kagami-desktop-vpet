<script setup>
// 主动互动的文字气泡（实施计划 §3.2）。
//
// - 绝对定位在桌宠窗口内、模型上方，**不改变窗口尺寸**；
// - 默认 5 秒后淡出，点它立刻关闭；
// - 同时只显示一条（新的进来会直接替换旧的，所以这里只管自己这一条的生命周期）；
// - 透明区域不挡模型：根节点 pointer-events: none，只有气泡本体 auto；
// - 与气泡菜单互斥不在这里管——显示条件挂在 App.vue（它拿着菜单状态机）。
//
// 两种消失方式要分开回报给 Rust：点掉 = 用户看见了，自己淡出 = 被忽略。
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { clampSpeechText } from '../composables/speechBubble.js'

const props = defineProps({
  text: { type: String, required: true },
  ttlMs: { type: Number, default: 5000 },
})
const emit = defineEmits(['expired', 'closed'])

// 淡出时长，与 style.css 里 .pet-speech-body 的 transition 对齐。
const FADE_MS = 220

const leaving = ref(false)
const shown = computed(() => clampSpeechText(props.text))
let expiryTimer = null
let fadeTimer = null

function leave(reason) {
  if (leaving.value) return
  leaving.value = true
  // 等淡出动画跑完再通知父组件卸载，否则气泡会「啪」地消失。
  fadeTimer = setTimeout(() => emit(reason), FADE_MS)
}

onMounted(() => {
  expiryTimer = setTimeout(() => leave('expired'), Math.max(1, props.ttlMs))
})
onBeforeUnmount(() => {
  clearTimeout(expiryTimer)
  clearTimeout(fadeTimer)
})
</script>

<template>
  <div class="pet-speech" :class="{ leaving }">
    <button type="button" class="pet-speech-body" @click.stop="leave('closed')">{{ shown }}</button>
  </div>
</template>
