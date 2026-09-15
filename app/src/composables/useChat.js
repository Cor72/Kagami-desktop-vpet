// 对话窗口的状态：当前会话、消息列表、发送、停止。
//
// 两个关键设计：
// 1. **按 50ms 节流刷新消息**（计划 §10 第 3 条）。模型的 delta 事件很密，
//    每个事件都改一次 DOM 会把主线程占满，抢走 Live2D 的帧预算——桌宠会明显卡。
// 2. **事件与命令返回值的到达顺序不定**，所以消息一律按 id 合并（见 chatMessages.js）。

import { computed, onMounted, onUnmounted, ref } from 'vue'
import {
  cancelStream, createSession, getMessages, listSessions,
  onChatStreamDelta, onChatStreamFailed, onChatStreamFinished, onChatStreamStarted, sendMessage,
} from '../api/chat.js'
import {
  appendUserMessage, applyDelta, ensureAssistantMessage, finishStream, removeMessage, storedToMessages,
} from './chatMessages.js'
import { createDeltaBatch } from './deltaBatch.js'

export function useChat() {
  const ready = ref(false)
  const sessionId = ref('')
  const messages = ref([])
  const sending = ref(false)
  const streamingId = ref('')
  const error = ref('')
  const notice = ref('')

  let disposed = false
  let unlisten
  let localSeq = 0

  // ---------- 50ms 节流 ----------
  // delta 事件很密，直接改 DOM 会抢走 Live2D 的帧预算（计划 §10 第 3 条）。
  const batch = createDeltaBatch({
    onFlush(entries) {
      let next = messages.value
      for (const [id, text] of entries) {
        next = applyDelta(next, id, text, { createdAt: Date.now() })
      }
      messages.value = next
    },
  })
  const flush = () => batch.flush()

  // ---------- 事件 ----------
  const forThisSession = payload => payload?.sessionId === sessionId.value

  function onStarted(payload) {
    if (disposed || !forThisSession(payload)) return
    flush()
    messages.value = ensureAssistantMessage(messages.value, payload.messageId, {
      createdAt: Date.now(),
    })
    streamingId.value = payload.messageId
  }

  function onDelta(payload) {
    if (disposed || !forThisSession(payload)) return
    if (!streamingId.value) streamingId.value = payload.messageId
    batch.push(payload.messageId, payload.text)
  }

  function onFinished(payload) {
    if (disposed || !forThisSession(payload)) return
    flush()
    messages.value = finishStream(messages.value, payload.messageId)
    if (streamingId.value === payload.messageId) streamingId.value = ''
  }

  function onFailed(payload) {
    if (disposed || !forThisSession(payload)) return
    flush()
    messages.value = finishStream(messages.value, payload.messageId, payload.error)
    if (streamingId.value === payload.messageId) streamingId.value = ''
    // 气泡里也会写原因；这里再提示一次，免得用户只看到一条空消息。
    notice.value = payload.error
  }

  // ---------- 会话 ----------
  async function loadLatest() {
    const sessions = await listSessions()
    if (disposed) return
    // 第一次打开还没有任何会话：直接建一个，省得用户先点「新建对话」。
    const latest = sessions[0] ?? (await createSession())
    if (disposed) return
    sessionId.value = latest.id
    messages.value = storedToMessages(await getMessages(latest.id))
  }

  onMounted(async () => {
    try {
      const stops = await Promise.all([
        onChatStreamStarted(onStarted),
        onChatStreamDelta(onDelta),
        onChatStreamFinished(onFinished),
        onChatStreamFailed(onFailed),
      ])
      if (disposed) {
        stops.forEach(stop => stop())
        return
      }
      // 一次订阅只留一个取消函数：它们是一起建立的，也会一起拆掉。
      unlisten = () => stops.forEach(stop => stop())
      await loadLatest()
      if (!disposed) ready.value = true
    } catch (cause) {
      if (!disposed) error.value = `打开对话失败：${String(cause)}`
    }
  })

  onUnmounted(() => {
    disposed = true
    batch.dispose()
    unlisten?.()
  })

  async function send(text) {
    const value = (text ?? '').trim()
    if (!value || !sessionId.value || sending.value || streamingId.value) return false
    error.value = ''
    notice.value = ''

    // 先乐观显示，再发命令：用户敲下回车就能看到自己的话。
    const localId = `local-${++localSeq}`
    messages.value = appendUserMessage(messages.value, {
      id: localId,
      text: value,
      createdAt: Date.now(),
    })
    sending.value = true
    try {
      const { messageId } = await sendMessage(sessionId.value, value)
      messages.value = ensureAssistantMessage(messages.value, messageId, {
        createdAt: Date.now(),
      })
      streamingId.value = messageId
      return true
    } catch (cause) {
      // 命令失败说明这条消息根本没进历史：撤掉它，并把文字还给输入框。
      messages.value = removeMessage(messages.value, localId)
      error.value = String(cause)
      return false
    } finally {
      if (!disposed) sending.value = false
    }
  }

  async function stop() {
    if (!sessionId.value) return
    try {
      await cancelStream(sessionId.value)
    } catch (cause) {
      if (!disposed) error.value = String(cause)
    }
  }

  async function startNewSession() {
    if (sending.value || streamingId.value) return false
    try {
      const session = await createSession()
      if (disposed) return false
      sessionId.value = session.id
      messages.value = []
      error.value = ''
      notice.value = ''
      return true
    } catch (cause) {
      error.value = String(cause)
      return false
    }
  }

  const busy = computed(() => sending.value || Boolean(streamingId.value))

  return { ready, busy, sending, streamingId, sessionId, messages, error, notice, send, stop, startNewSession }
}
