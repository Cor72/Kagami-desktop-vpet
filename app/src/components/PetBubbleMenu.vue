<script setup>
import { computed, nextTick, ref, watch } from 'vue'
import { ArrowLeft, Droplet, EyeOff, Frown, Laugh, Pin, Power, Settings, Smile } from '@lucide/vue'
import { getMenuItems } from '../composables/petMenu.js'

const props = defineProps({
  state: { type: String, required: true },
  busy: Boolean,
  alwaysOnTop: Boolean,
})
const emit = defineEmits(['select'])
const menu = ref(null)
const open = computed(() => props.state !== 'closed')
const items = computed(() => getMenuItems(props.state))
const icons = { 'arrow-left': ArrowLeft, droplet: Droplet, 'eye-off': EyeOff, frown: Frown, laugh: Laugh, pin: Pin, power: Power, settings: Settings, smile: Smile }

function focusFirst() { menu.value?.querySelector('button:not(:disabled)')?.focus({ preventScroll: true }) }
// 文字不再常显，改为原生 title 悬浮提示；同时作为无障碍名称，常显的 aria-label 去掉。
function tooltip(item) {
  if (item.id === 'always-on-top') return props.alwaysOnTop ? '取消置顶' : '保持置顶'
  return item.label
}
watch(open, async value => {
  if (!value) return
  await nextTick()
  focusFirst()
})
function leaveLayer(element) { element.inert = true }
function navigate(event) {
  if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return
  event.preventDefault()
  const buttons = [...menu.value.querySelectorAll('button:not(:disabled)')]
  const index = buttons.indexOf(document.activeElement)
  const next = event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length - 1 : (index + (event.key === 'ArrowDown' ? 1 : -1) + buttons.length) % buttons.length
  buttons[next]?.focus()
}
</script>

<template>
  <!-- 菜单是桌宠窗口内的一块普通 DOM，始终竖排在画面右侧，不改变窗口尺寸。 -->
  <nav v-if="open" ref="menu" class="bubble-menu" :aria-label="state === 'root' ? '桌宠一级菜单' : '表情菜单'" @pointerdown.stop @contextmenu.prevent.stop @keydown="navigate">
    <Transition name="menu-layer" mode="out-in" @before-leave="leaveLayer" @after-enter="focusFirst">
      <div :key="state" class="bubble-layer">
        <button v-for="(item, index) in items" :key="item.id" class="bubble-item" :class="{ selected: item.id === 'always-on-top' && alwaysOnTop, danger: item.danger }" :style="{ '--slot': index }" type="button" :disabled="busy" :title="tooltip(item)" :aria-label="tooltip(item)" :aria-pressed="item.id === 'always-on-top' ? alwaysOnTop : undefined" :aria-haspopup="item.submenu ? 'menu' : undefined" @click.stop="emit('select', item.id)">
          <span class="bubble-orb"><component :is="icons[item.icon]" :size="20" :stroke-width="1.65" aria-hidden="true" /><span v-if="item.id === 'always-on-top' && alwaysOnTop" class="bubble-dot" /></span>
        </button>
      </div>
    </Transition>
  </nav>
</template>
