<script setup>
import { nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { resolveResource } from '@tauri-apps/api/path'
import { createPetController } from '../live2d/controller.js'

const props = defineProps({ expressionRequest: { type: Object, default: null } })
const canvas = ref(null)
const canvasKey = ref(0)
const stage = ref(null)
const loading = ref(false)
const error = ref('')
const modelDirectory = ref('assets/models/yachiyo')
const appliedExpression = ref('')
const isDev = import.meta.env.DEV

let controller
let observer
let abortController
let disposed = false

async function applyLatestExpression() {
  if (!controller || !props.expressionRequest) return
  const { name } = props.expressionRequest
  try {
    await controller.setExpression(name)
    if (!disposed) appliedExpression.value = name
  } catch (cause) {
    if (!disposed) error.value = String(cause)
  }
}

async function loadModel() {
  if (loading.value || disposed) return
  loading.value = true
  error.value = ''
  appliedExpression.value = ''
  controller?.destroy()
  controller = null
  abortController = new AbortController()

  try {
    // Pixi 销毁时会释放 WebGL 上下文，重试必须换一个新的 Canvas。
    canvasKey.value++
    await nextTick()
    if (disposed) return
    modelDirectory.value = await resolveResource('assets/models/yachiyo')
    const next = await createPetController({
      canvas: canvas.value,
      modelDirectory: modelDirectory.value,
      signal: abortController.signal,
    })
    if (disposed) {
      next.destroy()
      return
    }
    controller = next
    await applyLatestExpression()
  } catch (cause) {
    if (!disposed) error.value = String(cause)
  } finally {
    if (!disposed) loading.value = false
  }
}

// 加载中只保留 props 中最新的请求；就绪后应用一次，不积压旧表情。
watch(() => props.expressionRequest, applyLatestExpression)

onMounted(() => {
  observer = new ResizeObserver(([entry]) => {
    controller?.resize(entry.contentRect.width, entry.contentRect.height)
  })
  observer.observe(stage.value)
  void loadModel()
})

onUnmounted(() => {
  disposed = true
  abortController?.abort()
  observer?.disconnect()
  controller?.destroy()
  controller = null
})
</script>

<template>
  <section class="pet-view" aria-label="八千代模型">
    <div ref="stage" class="pet-stage">
      <canvas :key="canvasKey" ref="canvas" aria-label="八千代 Live2D 模型" />
      <div v-if="loading || error" class="stage-message" :role="error ? 'alert' : 'status'">
        <p>{{ loading ? '正在加载八千代…' : '模型加载或表情切换失败' }}</p>
        <template v-if="error">
          <p>{{ error }}</p>
          <code>{{ modelDirectory }}</code>
          <button type="button" @click="loadModel">重试加载</button>
        </template>
      </div>
    </div>
    <div class="model-toolbar">
      <span role="status">
        {{ loading ? '加载中' : error ? '请检查上方错误' : `模型已就绪 · 30 FPS${appliedExpression ? ` · ${appliedExpression}` : ''}` }}
      </span>
      <button v-if="isDev" class="text-button" type="button" :disabled="loading" @click="loadModel">重新加载</button>
    </div>
  </section>
</template>
