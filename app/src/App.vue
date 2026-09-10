<script setup>
import { ref } from 'vue'
import DevPanel from './components/DevPanel.vue'
import PetStage from './components/PetStage.vue'
import { usePet } from './composables/usePet.js'

// 事件订阅属于整个应用，发布版也要接收 Rust 发来的表情事件。
const pet = usePet()
const { expressionRequest, settings, ready, settingsBusy, error, setVisible, quit } = pet
const isDev = import.meta.env.DEV
const panelOpen = ref(false)
</script>

<template>
  <main :class="{ 'panel-open': panelOpen }">
    <PetStage :expression-request="expressionRequest" :settings="settings" @error="error = $event" />
    <div class="window-controls hover-controls" aria-label="桌宠控制">
      <button v-if="isDev" class="text-button" :aria-expanded="panelOpen" @click="panelOpen = !panelOpen">
        {{ panelOpen ? '收起联调' : '联调' }}
      </button>
      <button class="text-button" :disabled="!ready || settingsBusy" @click="setVisible(false)">隐藏</button>
      <button class="text-button" @click="quit">退出</button>
    </div>
    <DevPanel v-if="isDev && panelOpen" :pet="pet" />
    <p v-if="error && !panelOpen" class="floating-error" role="alert">{{ error }}</p>
  </main>
</template>
