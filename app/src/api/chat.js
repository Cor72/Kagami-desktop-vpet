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

// ---------- 工作区（Agent 模式）----------
//
// 「添加文件夹」会弹系统目录选择器，但**选择器是在 Rust 里调的**：前端不需要
// 任何文件系统或对话框权限（计划 §10 第 1 条）。拖进来的文件一律是只读条目。

export const listWorkspaceEntries = () => invoke('list_workspace_entries')
export const addWorkspaceDir = () => invoke('add_workspace_dir')
export const addWorkspaceFiles = paths => invoke('add_workspace_files', { paths })
export const removeWorkspaceEntry = id => invoke('remove_workspace_entry', { id })

// ---------- 写入确认 ----------
//
// 这两条是「点「应用」才真写」的唯一入口：用户点之前，磁盘上什么都没发生。

export const applyPendingWrite = requestId => invoke('apply_pending_write', { requestId })
export const rejectPendingWrite = requestId => invoke('reject_pending_write', { requestId })

// ---------- 流式事件 ----------

export const onChatStreamStarted = handler =>
  listen('chat-stream-started', event => handler(event.payload))
export const onChatStreamDelta = handler =>
  listen('chat-stream-delta', event => handler(event.payload))
export const onChatStreamFinished = handler =>
  listen('chat-stream-finished', event => handler(event.payload))
export const onChatStreamFailed = handler =>
  listen('chat-stream-failed', event => handler(event.payload))

// ---------- Agent 模式的事件 ----------

// 一次工具调用开始：`label` 就是卡片上那句「正在读取 xxx」。
export const onChatToolCall = handler =>
  listen('chat-tool-call', event => handler(event.payload))
// 一次工具调用的结果：`summary` 是「读完了」「没写成」这类短句。
export const onChatToolResult = handler =>
  listen('chat-tool-result', event => handler(event.payload))
// 等用户确认的写入：`diff` 是 unified 格式原文，前端按行前缀上色。
export const onChatWriteRequest = handler =>
  listen('chat-write-request', event => handler(event.payload))
// 工作区条目变了（自己加的、或者别的窗口改的）。
export const onWorkspaceChanged = handler =>
  listen('workspace-changed', event => handler(event.payload))
