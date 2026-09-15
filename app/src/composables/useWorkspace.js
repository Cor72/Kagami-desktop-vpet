// Agent 模式的工作区状态：条目清单、添加、删除、拖拽投放。
//
// 与其他 composable 同一套约定：先订阅事件再读初始值；命令返回的快照直接拿来更新界面。
// 拖拽只在 Agent 模式生效——聊天模式下拖进来一个文件就悄悄授权，用户会莫名其妙。

import { onMounted, onUnmounted, ref } from 'vue'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import {
  addWorkspaceDir, addWorkspaceFiles, listWorkspaceEntries, onWorkspaceChanged,
  removeWorkspaceEntry,
} from '../api/chat.js'
import { acceptEntries, pickFilesOnly } from './workspace.js'

export function useWorkspace({ active = () => true } = {}) {
  const ready = ref(false)
  const entries = ref([])
  const busy = ref(false)
  /** 有东西正悬在窗口上——给拖拽区一个高亮。 */
  const dragging = ref(false)
  const error = ref('')
  const notice = ref('')

  let disposed = false
  let stops = []

  const accept = incoming => {
    if (!disposed) entries.value = acceptEntries(entries.value, incoming)
  }

  async function run(command) {
    if (busy.value) return false
    busy.value = true
    error.value = ''
    notice.value = ''
    try {
      accept(await command())
      return true
    } catch (cause) {
      if (!disposed) error.value = String(cause)
      return false
    } finally {
      if (!disposed) busy.value = false
    }
  }

  onMounted(async () => {
    try {
      const unlisten = await onWorkspaceChanged(accept)
      if (disposed) {
        unlisten()
        return
      }
      stops.push(unlisten)

      // 拖拽事件是窗口级的：只在 Agent 模式下处理。
      const stopDrag = await getCurrentWebview().onDragDropEvent(event => {
        if (disposed || !active()) return
        const { type } = event.payload
        if (type === 'enter' || type === 'over') {
          dragging.value = true
          return
        }
        dragging.value = false
        if (type === 'drop') void dropFiles(pickFilesOnly(event.payload.paths))
      })
      if (disposed) {
        stopDrag()
        return
      }
      stops.push(stopDrag)

      accept(await listWorkspaceEntries())
      if (!disposed) ready.value = true
    } catch (cause) {
      if (!disposed) error.value = `读取工作区失败：${String(cause)}`
    }
  })

  onUnmounted(() => {
    disposed = true
    stops.forEach(stop => stop())
    stops = []
  })

  /** 系统目录选择器 → 目录条目（可读写，改动仍要过 diff 确认）。 */
  async function addDir() {
    await run(addWorkspaceDir)
  }

  /** 拖进来的文件 → 只读文件条目。目录不走这条路（那会绕开明确的授权动作）。 */
  async function dropFiles(paths) {
    if (!paths.length) return
    await run(() => addWorkspaceFiles(paths))
  }

  async function remove(id) {
    await run(() => removeWorkspaceEntry(id))
  }

  return { ready, entries, busy, dragging, error, notice, addDir, dropFiles, remove }
}
