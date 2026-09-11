<script setup>
import { nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { resolveResource } from '@tauri-apps/api/path'
import { createPetController } from '../live2d/controller.js'
import { onPetModelReload, startPetDragging } from '../api/pet.js'
import { createPetGesture } from '../interactions/petGesture.js'

const props = defineProps({
  expressionRequest: { type: Object, default: null },
  renderPolicy: { type: Object, required: true },
  beforeDrag: { type: Function, default: () => {} },
})
const emit = defineEmits(['error', 'activate', 'context'])
const canvas = ref(null)
const canvasKey = ref(0)
const stage = ref(null)
const loading = ref(false)
const error = ref('')
const modelDirectory = ref('assets/models/yachiyo')
const appliedExpression = ref('')
const gesture = createPetGesture()
let pressed = false
let stopReload

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
    applyRenderPolicy()
    await applyLatestExpression()
  } catch (cause) {
    if (!disposed) error.value = String(cause)
  } finally {
    if (!disposed) loading.value = false
  }
}

// 加载中只保留 props 中最新的请求；就绪后应用一次，不积压旧表情。
watch(() => props.expressionRequest, applyLatestExpression)
function applyRenderPolicy() {
  if (!controller) return
  controller.setMaxFps(props.renderPolicy.maxFps)
  controller.setRunning(props.renderPolicy.running)
}
// 隐藏时立即停掉同一个 ticker，恢复时继续使用同一份模型。
watch(() => props.renderPolicy, applyRenderPolicy, { flush: 'sync' })

async function dragWindow() {
  try {
    const pendingClose = props.beforeDrag()
    if (pendingClose) await pendingClose
    if (pressed && !disposed) await startPetDragging()
  }
  catch (cause) { emit('error', `拖动窗口失败：${String(cause)}`) }
  finally { cancelGesture() }
}

function cancelGesture() { pressed = false; gesture.cancel() }
function pointerDown(event) {
  if (event.button !== 0) return
  event.preventDefault()
  pressed = true
  gesture.pointerDown(event)
  event.currentTarget.setPointerCapture(event.pointerId)
}
function pointerMove(event) {
  if (gesture.pointerMove(event) === 'drag') void dragWindow()
}
function pointerUp(event) {
  pressed = false
  if (gesture.pointerUp(event) === 'click') emit('activate')
  if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId)
}
function contextMenu() { cancelGesture(); emit('context') }
function focus() { canvas.value?.focus({ preventScroll: true }) }
defineExpose({ focus })

onMounted(() => {
  window.addEventListener('blur', cancelGesture)
  if (import.meta.env.DEV) {
    onPetModelReload(() => { void loadModel() }).then(stop => {
      if (disposed) stop()
      else stopReload = stop
    }).catch(cause => emit('error', `订阅重新加载失败：${String(cause)}`))
  }
  observer = new ResizeObserver(([entry]) => {
    controller?.resize(entry.contentRect.width, entry.contentRect.height)
  })
  observer.observe(stage.value)
  void loadModel()
})

onUnmounted(() => {
  disposed = true
  cancelGesture()
  window.removeEventListener('blur', cancelGesture)
  stopReload?.()
  abortController?.abort()
  observer?.disconnect()
  controller?.destroy()
  controller = null
})
</script>

<template>
  <section class="pet-view" aria-label="八千代模型">
    <div ref="stage" class="pet-stage">
      <canvas :key="canvasKey" ref="canvas" tabindex="0" role="button" aria-label="八千代：单击随机表情，按住拖动，右键打开菜单" @pointerdown="pointerDown" @pointermove="pointerMove" @pointerup="pointerUp" @pointercancel="cancelGesture" @contextmenu.prevent.stop="contextMenu" @keydown.enter.prevent="emit('activate')" @keydown.space.prevent="emit('activate')" @keydown.shift.f10.prevent="contextMenu" />
      <div v-if="loading || error" class="stage-message" :role="error ? 'alert' : 'status'">
        <p>{{ loading ? '正在加载八千代…' : '模型加载或表情切换失败' }}</p>
        <template v-if="error">
          <p>{{ error }}</p>
          <code>{{ modelDirectory }}</code>
          <button type="button" @click="loadModel">重试加载</button>
        </template>
      </div>
    </div>
  </section>
</template>
