<script setup>
import { PET_EXPRESSIONS } from '../api/pet.js'

const labels = { smile: '微笑', squint: '眯眼', tears: '泪眼', teardrop: '泪滴' }
// 状态由 App.vue 创建并共享，面板和模型不重复注册事件监听。
const props = defineProps({ pet: { type: Object, required: true } })
const { ready, sending, requestStatus, lastExpression, receivedCount, error, sendExpression } = props.pet
const { settings, settingsBusy, setMaxFps, setAlwaysOnTop } = props.pet
</script>

<template>
  <section class="dev-panel" aria-labelledby="bridge-title">
    <h2 id="bridge-title">表情联调 · Vue ↔ Rust</h2>
    <p class="connection" role="status">
      {{ ready ? '事件监听已就绪' : error ? '事件监听未就绪' : '等待事件监听就绪…' }}
    </p>
    <div v-if="settings" class="settings-controls">
      <label>
        <input type="checkbox" :checked="settings.alwaysOnTop" :disabled="settingsBusy" @change="setAlwaysOnTop($event.target.checked)">
        保持置顶
      </label>
      <label>帧率
        <select :value="settings.maxFps" :disabled="settingsBusy" @change="setMaxFps(Number($event.target.value))">
          <option :value="30">30 FPS</option>
          <option :value="15">15 FPS</option>
        </select>
      </label>
      <small>设置版本 {{ settings.revision }} · {{ settings.visible ? '显示' : '隐藏' }}</small>
    </div>

    <div class="expression-buttons">
      <button
        v-for="name in PET_EXPRESSIONS"
        :key="name"
        type="button"
        :disabled="!ready || sending"
        @click="sendExpression(name)"
      >
        {{ labels[name] }}
      </button>
    </div>
    <button class="secondary" type="button" :disabled="!ready || sending" @click="sendExpression('unknown')">
      测试错误：unknown
    </button>

    <dl class="bridge-status" aria-live="polite">
      <div>
        <dt>命令返回</dt>
        <dd>{{ requestStatus }}</dd>
      </div>
      <div>
        <dt>最近一次事件 · 共 {{ receivedCount }} 次</dt>
        <dd>{{ lastExpression ? `收到 Rust 事件：${lastExpression}` : '尚未收到事件' }}</dd>
      </div>
    </dl>
    <p v-if="error" class="error" role="alert">{{ error }}</p>
  </section>
</template>
