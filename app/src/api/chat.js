// 对话与 AI 设置的 IPC 入口：命令名与事件名只写在这里，组件里不出现裸字符串。
// 约定与 `api/pet.js` 一致：Vue → Rust 用 invoke，参数对象用 camelCase；
// Rust → Vue 用 listen，订阅函数返回「取消订阅」的函数。
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

export const CHAT_WINDOW_LABEL = 'chat'

// ---------- 窗口 ----------

// 右键桌宠 → 气泡菜单「对话」：打开（或聚焦）对话窗口。
// 窗口是单例，重复调用只会把已经存在的那个叫到前面来。
export const openChatWindow = () => invoke('open_chat_window')

// ---------- AI 配置与 API Key ----------
//
// 注意：这里**没有**任何「取回 Key 全文」的命令，以后也不要有。
// 前端能拿到的只有 `hasKey` 与 `keyMask`（如 `sk-****1234`）。

export const getAiConfig = () => invoke('get_ai_config')
export const setAiConfig = patch => invoke('set_ai_config', { patch })
export const setApiKey = (provider, key) => invoke('set_api_key', { provider, key })
export const clearApiKey = provider => invoke('clear_api_key', { provider })
// 真的发一次最小请求去探活，返回 { ok, message }。
export const testAiConnection = () => invoke('test_ai_connection')
export const onAiConfigChanged = handler =>
  listen('ai-config-changed', event => handler(event.payload))

// ---------- 会话与消息 ----------

export const createSession = () => invoke('create_session')
export const listSessions = () => invoke('list_sessions')
export const getMessages = sessionId => invoke('get_messages', { sessionId })
// 立刻返回 { messageId }；回答通过下面的流式事件陆续到达。
export const sendMessage = (sessionId, text) => invoke('send_message', { sessionId, text })
export const cancelStream = sessionId => invoke('cancel_stream', { sessionId })

// ---------- 流式事件 ----------

export const onChatStreamStarted = handler =>
  listen('chat-stream-started', event => handler(event.payload))
export const onChatStreamDelta = handler =>
  listen('chat-stream-delta', event => handler(event.payload))
export const onChatStreamFinished = handler =>
  listen('chat-stream-finished', event => handler(event.payload))
export const onChatStreamFailed = handler =>
  listen('chat-stream-failed', event => handler(event.payload))
