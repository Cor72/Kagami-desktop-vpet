<script setup>
import DevPanel from './components/DevPanel.vue'
import PetStage from './components/PetStage.vue'
import { usePet } from './composables/usePet.js'

// 事件订阅属于整个应用，发布版也要接收 Rust 发来的表情事件。
const pet = usePet()
const { expressionRequest } = pet
const isDev = import.meta.env.DEV
</script>

<template>
  <main>
    <h1>八千代桌宠</h1>
    <PetStage :expression-request="expressionRequest" />
    <DevPanel v-if="isDev" :pet="pet" />
  </main>
</template>
