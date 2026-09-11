<script setup>
import { computed, nextTick, onMounted, onUnmounted, ref, shallowRef } from 'vue'
import PetStage from './components/PetStage.vue'
import PetBubbleMenu from './components/PetBubbleMenu.vue'
import { usePet } from './composables/usePet.js'
import { getMenuItems, pickExpression } from './composables/petMenu.js'
import { createMenuController } from './composables/menuController.js'
import { onPetMenuLayoutChanged, openPetSettings, PET_EXPRESSIONS, setPetMenuOpen } from './api/pet.js'

const pet = usePet()
const { expressionRequest, renderPolicy, ready, sending, settings, settingsBusy, error, lastExpression, sendExpression, setVisible, setAlwaysOnTop, quit } = pet
const stage = ref(null)
const menu = shallowRef({ state: 'closed', busy: false, layout: { side: 'right', modelOffsetX: 0 } })
const menuReady = ref(false)
const actionBusy = ref(false)
const busy = computed(() => !ready.value || !menuReady.value || menu.value.busy || actionBusy.value || sending.value || settingsBusy.value)
const isDev = import.meta.env.DEV
let disposed = false
let unlisten
const controller = createMenuController({
  setOpen: setPetMenuOpen,
  onChange: value => { if (!disposed) menu.value = value },
  onError: cause => { if (!disposed) error.value = `调整菜单窗口失败：${String(cause)}` },
})
async function changeMenu(action, restoreFocus = true) {
  if (!menuReady.value) return
  await controller.dispatch(action)
  if (restoreFocus && menu.value.state === 'closed' && !disposed) {
    await nextTick()
    stage.value?.focus()
  }
}
const closeMenu = () => changeMenu('close')
const beforeDrag = () => menu.value.state !== 'closed' || menu.value.busy ? changeMenu('close', false) : undefined
async function activatePet() {
  await closeMenu()
  const name = pickExpression(PET_EXPRESSIONS, lastExpression.value)
  if (name) await sendExpression(name)
}
async function selectItem(id) {
  if (busy.value || !getMenuItems(menu.value.state).some(item => item.id === id)) return
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
onMounted(async () => {
  window.addEventListener('blur', blur)
  window.addEventListener('keydown', keydown)
  try {
    const stop = await onPetMenuLayoutChanged(layout => controller.reset(layout))
    if (disposed) { stop(); return }
    unlisten = stop
    controller.reset(await setPetMenuOpen(false))
    if (!disposed) menuReady.value = true
  } catch (cause) { if (!disposed) error.value = `菜单初始化失败：${String(cause)}` }
})
onUnmounted(() => {
  disposed = true
  window.removeEventListener('blur', blur)
  window.removeEventListener('keydown', keydown)
  unlisten?.()
  void controller.dispatch('close')
})
</script>

<template>
  <main class="pet-shell" @pointerdown="pointerOutside" @contextmenu.prevent>
    <div class="pet-anchor" :class="{ 'development-stage': isDev }" :style="{ left: `${menu.layout.modelOffsetX}px` }">
      <PetStage ref="stage" :expression-request="expressionRequest" :render-policy="renderPolicy" :before-drag="beforeDrag" @activate="activatePet" @context="changeMenu('context')" @error="error = $event" />
      <p v-if="error" class="floating-error" role="alert">{{ error }}</p>
    </div>
    <PetBubbleMenu :state="menu.state" :side="menu.layout.side" :menu-top="menu.layout.menuTop" :menu-height="menu.layout.menuHeight" :menu-offset-x="menu.layout.menuOffsetX" :busy="busy" :always-on-top="settings?.alwaysOnTop ?? false" @select="selectItem" />
  </main>
</template>
