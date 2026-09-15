<script setup>
// 写入确认卡片：红绿 diff + 「应用」/「拒绝」。
//
// **没有 diff 的确认等于盲签**（实施计划 §9 阶段 C）：这张卡片存在的全部意义，
// 就是让用户在改动落盘之前看清会改掉哪几行。
import { computed } from 'vue'
import { Check, X } from '@lucide/vue'
import { diffLines } from '../../composables/chatMessages.js'

const props = defineProps({
  request: { type: Object, required: true },
  busy: Boolean,
})

const emit = defineEmits(['apply', 'reject'])

// 最后那个空行是 diff 末尾的换行留下的，不显示。
const lines = computed(() => {
  const all = diffLines(props.request.diff)
  while (all.length && all[all.length - 1].kind === 'blank') all.pop()
  return all
})
const pending = computed(() => props.request.state === 'pending')
const stateText = computed(() => {
  switch (props.request.state) {
    case 'applied':
      return '已经改好了'
    case 'rejected':
      return '没有改，文件保持原样'
    case 'failed':
      return `没写成：${props.request.message}`
    case 'expired':
      return '这一步已经结束了，没有改动'
    default:
      return ''
  }
})
</script>

<template>
  <div class="write-row">
    <section class="write-card" :class="`state-${request.state}`" aria-label="改动确认">
      <header class="write-head">
        <span class="write-path" :title="request.path">{{ request.path }}</span>
      </header>
      <p v-if="request.reason" class="write-reason">她说：{{ request.reason }}</p>
      <!-- 每一行是一个 display:block 的 span：不依赖模板里的换行符，缩进也不会跑掉。 -->
      <pre class="write-diff"><code><span
        v-for="line in lines"
        :key="line.index"
        class="diff-line"
        :class="`diff-${line.kind}`"
      >{{ line.text || ' ' }}</span></code></pre>
      <footer class="write-actions">
        <button
          type="button"
          class="write-apply"
          :disabled="!pending || busy"
          @click="emit('apply', request.requestId)"
        ><Check :size="13" aria-hidden="true" />应用</button>
        <button
          type="button"
          class="write-reject"
          :disabled="!pending || busy"
          @click="emit('reject', request.requestId)"
        ><X :size="13" aria-hidden="true" />拒绝</button>
        <span v-if="stateText" class="write-state" role="status">{{ stateText }}</span>
      </footer>
    </section>
  </div>
</template>
