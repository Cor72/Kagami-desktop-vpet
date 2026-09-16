<script setup>
// 工作区栏：只读文件画一把锁，目录写「可读写」。
// 「添加文件夹」走系统目录选择器；拖进来的文件一律只读。
import { FolderPlus, Folder, FileText, Lock, X } from '@lucide/vue'
import { accessLabel, countLabel, kindLabel, isReadOnly } from '../../composables/workspace.js'

defineProps({
  entries: { type: Array, required: true },
  busy: Boolean,
  dragging: Boolean,
  error: { type: String, default: '' },
})

const emit = defineEmits(['add-dir', 'remove'])
</script>

<template>
  <section class="workspace" :class="{ dragging }" aria-label="工作区">
    <header class="workspace-head">
      <span class="workspace-title">工作区</span>
      <span class="workspace-count">{{ countLabel(entries) }}</span>
    </header>

    <ul v-if="entries.length" class="workspace-list">
      <li v-for="entry in entries" :key="entry.id" class="workspace-item">
        <span class="workspace-kind" aria-hidden="true">
          <FileText v-if="isReadOnly(entry)" :size="14" />
          <Folder v-else :size="14" />
        </span>
        <span class="workspace-name" :title="entry.path">{{ entry.label }}</span>
        <span class="workspace-access" :class="{ readonly: isReadOnly(entry) }">
          <Lock v-if="isReadOnly(entry)" :size="10" aria-hidden="true" />{{ accessLabel(entry) }}·{{ kindLabel(entry) }}
        </span>
        <button
          type="button"
          class="workspace-remove"
          :aria-label="`移除 ${entry.label}`"
          :disabled="busy"
          @click="emit('remove', entry.id)"
        ><X :size="12" aria-hidden="true" /></button>
      </li>
    </ul>
    <p v-else class="workspace-empty">
      还没有给八千代任何文件。加一个文件夹（可读写），或者把文件拖到这里（只读）。
    </p>

    <div class="workspace-drop">
      <button type="button" class="text-button" :disabled="busy" @click="emit('add-dir')">
        <FolderPlus :size="13" aria-hidden="true" />添加文件夹
      </button>
      <span class="workspace-hint">{{ dragging ? '松手就加进来（只读）' : '把文件拖到这里' }}</span>
    </div>

    <p v-if="error" class="workspace-error" role="alert">{{ error }}</p>
  </section>
</template>
