<script setup>
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import PetStage from './components/PetStage.vue'
import PetBubbleMenu from './components/PetBubbleMenu.vue'
import PetSpeechBubble from './components/PetSpeechBubble.vue'
import { usePet } from './composables/usePet.js'
import { getMenuItems, pickExpression, transitionMenu } from './composables/petMenu.js'
import { speechAllowed } from './composables/speechBubble.js'
import { onProactiveSpeak, openPetSettings, PET_EXPRESSIONS, proactiveDismiss } from './api/pet.js'
import { openChatWindow } from './api/chat.js'

const pet = usePet()
const { expressionRequest, renderPolicy, ready, sending, settings, settingsBusy, error, lastExpression, sendExpression, setVisible, setAlwaysOnTop, quit } = pet
const stage = ref(null)
// 菜单状态是纯前端状态：不再请求 Rust 调整窗口尺寸，也就没有"等原生布局回传"的时序问题。
const menuState = ref('closed')
const actionBusy = ref(false)
const busy = computed(() => !ready.value || actionBusy.value || sending.value || settingsBusy.value)
const isDev = import.meta.env.DEV
// 主动互动的气泡：同一时刻只有一条，`seq` 用来让新的一句替换旧的那条组件实例。
const bubble = ref(null)
let bubbleSeq = 0
let stopProactive = null
let disposed = false

// 气泡的显示条件与菜单状态挂在同一处（实施计划 §3.2）：
// 菜单打开时不显示气泡，也不要在这里再存一份菜单状态。
const bubbleVisible = computed(() => bubble.value !== null && speechAllowed(menuState.value))

// 清掉当前气泡。`report` 为假表示「这一条根本没露过面」，不参与「连续被忽略」计数。
function dropBubble({ acknowledged = false, report = true } = {}) {
  const current = bubble.value
  if (!current) return
  bubble.value = null
  if (report && !disposed) {
    // 回报失败不影响气泡本身：Rust 侧只是少记一次忽略。
    proactiveDismiss(current.id, acknowledged).catch(cause => console.error('[Vue] 气泡回报失败:', cause))
  }
}

function showBubble(payload) {
  // 菜单开着时丢掉新来的气泡：它没有露过面，不参与「连续被忽略」计数。
  if (!speechAllowed(menuState.value)) return
  // 新的一句直接替换旧的，旧的那条算被忽略（用户没点它）。
  dropBubble()
  bubbleSeq += 1
  bubble.value = { seq: bubbleSeq, id: payload.id, text: payload.text, ttlMs: payload.ttlMs }
}

// 菜单一打开就把气泡收掉，免得两者叠在一起。
watch(menuState, state => {
  if (state !== 'closed') dropBubble()
})
// 桌宠被隐藏时把气泡也收掉：窗口再显示出来时不该挂着一句过期的话。
watch(() => settings.value?.visible, visible => {
  if (visible === false) dropBubble({ report: false })
})

async function changeMenu(action, restoreFocus = true) {
  menuState.value = transitionMenu(menuState.value, action)
  if (restoreFocus && menuState.value === 'closed' && !disposed) {
    await nextTick()
    stage.value?.focus()
  }
}
const closeMenu = () => changeMenu('close')
const beforeDrag = () => menuState.value !== 'closed' ? changeMenu('close', false) : undefined
async function activatePet() {
  await closeMenu()
  const name = pickExpression(PET_EXPRESSIONS, lastExpression.value)
  if (name) await sendExpression(name)
}
async function selectItem(id) {
  if (busy.value || !getMenuItems(menuState.value).some(item => item.id === id)) return
  if (id === 'expressions' || id === 'back') { await changeMenu(id); return }
  actionBusy.value = true
  error.value = ''
  try {
    if (PET_EXPRESSIONS.includes(id)) {
      if (await sendExpression(id)) await closeMenu()
    } else if (id === 'chat') {
      // 对话窗口是独立窗口；菜单先收起，免得挡住模型。
      // 用 closeMenu(false) 不把焦点抢回画布——窗口切换由系统处理。
      await openChatWindow()
      await changeMenu('close', false)
    } else if (id === 'settings') {
      await openPetSettings()
      await changeMenu('close', false)
    } else if (id === 'always-on-top') {
      await setAlwaysOnTop(!settings.value?.alwaysOnTop)
    } else if (id === 'hide') {
      if (await setVisible(false)) await changeMenu('close', false)
    } else if (id === 'quit') { await quit() }
  } catch (cause) { error.value = String(cause) }
  finally { actionBusy.value = false }
}
function pointerOutside(event) {
  if (event.button === 0 && !event.target.closest('.bubble-menu, canvas, .stage-message')) void closeMenu()
}
function keydown(event) {
  if (event.key === 'Escape') { event.preventDefault(); void changeMenu('escape') }
}
function blur() { void changeMenu('close', false) }
onMounted(() => {
  window.addEventListener('blur', blur)
  window.addEventListener('keydown', keydown)
  // 订阅要等 Rust 侧的采样结果；组件可能在订阅完成前就被卸载。
  onProactiveSpeak(payload => {
    if (!disposed) showBubble(payload)
  })
    .then(stop => {
      if (disposed) stop()
      else stopProactive = stop
    })
    .catch(cause => {
      // 订阅失败只影响主动气泡，不影响桌宠本身。
      if (!disposed) error.value = `主动互动订阅失败：${String(cause)}`
    })
})
onUnmounted(() => {
  disposed = true
  window.removeEventListener('blur', blur)
  window.removeEventListener('keydown', keydown)
  stopProactive?.()
})
</script>

<template>
  <main class="pet-shell" @pointerdown="pointerOutside" @contextmenu.prevent>
    <div class="pet-anchor" :class="{ 'development-stage': isDev }">
      <PetStage ref="stage" :expression-request="expressionRequest" :render-policy="renderPolicy" :before-drag="beforeDrag" @activate="activatePet" @context="changeMenu('context')" @error="error = $event" />
      <PetBubbleMenu :state="menuState" :busy="busy" :always-on-top="settings?.alwaysOnTop ?? false" @select="selectItem" />
      <PetSpeechBubble v-if="bubbleVisible" :key="bubble.seq" :text="bubble.text" :ttl-ms="bubble.ttlMs" @expired="dropBubble()" @closed="dropBubble({ acknowledged: true })" />
      <p v-if="error" class="floating-error" role="alert">{{ error }}</p>
    </div>
  </main>
</template>
