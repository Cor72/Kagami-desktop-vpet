<script setup>
import { computed, nextTick, onMounted, onUnmounted, ref } from 'vue'
import PetStage from './components/PetStage.vue'
import PetBubbleMenu from './components/PetBubbleMenu.vue'
import { usePet } from './composables/usePet.js'
import { getMenuItems, pickExpression, transitionMenu } from './composables/petMenu.js'
import { openPetSettings, PET_EXPRESSIONS } from './api/pet.js'

const pet = usePet()
const { expressionRequest, renderPolicy, ready, sending, settings, settingsBusy, error, lastExpression, sendExpression, setVisible, setAlwaysOnTop, quit } = pet
const stage = ref(null)
// 菜单状态是纯前端状态：不再请求 Rust 调整窗口尺寸，也就没有"等原生布局回传"的时序问题。
const menuState = ref('closed')
const actionBusy = ref(false)
const busy = computed(() => !ready.value || actionBusy.value || sending.value || settingsBusy.value)
const isDev = import.meta.env.DEV
let disposed = false

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
})
onUnmounted(() => {
  disposed = true
  window.removeEventListener('blur', blur)
  window.removeEventListener('keydown', keydown)
})
</script>

<template>
  <main class="pet-shell" @pointerdown="pointerOutside" @contextmenu.prevent>
    <div class="pet-anchor" :class="{ 'development-stage': isDev }">
      <PetStage ref="stage" :expression-request="expressionRequest" :render-policy="renderPolicy" :before-drag="beforeDrag" @activate="activatePet" @context="changeMenu('context')" @error="error = $event" />
      <PetBubbleMenu :state="menuState" :busy="busy" :always-on-top="settings?.alwaysOnTop ?? false" @select="selectItem" />
      <p v-if="error" class="floating-error" role="alert">{{ error }}</p>
    </div>
  </main>
</template>
