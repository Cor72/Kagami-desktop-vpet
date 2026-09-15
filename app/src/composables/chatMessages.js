// 消息列表的纯函数操作。
//
// 单独抽出来的两个理由：
// 1. 流式拼接是最容易出错的逻辑，写成纯函数就能用 `node --test` 直接覆盖；
// 2. 事件与命令返回值**谁先到都有可能**——`chat-stream-delta` 可能比
//    `send_message` 的返回值先到。这里用「按 id 找，找不到就补一条」统一处理，
//    调用方不需要关心时序。
//
// 阶段 C 多了一类内容：挂在助手消息上的**工具卡片**与**写入确认卡片**。
// 它们也按 messageId 归位，所以同样用「找不到就补一条」的写法。

/** 一条消息的初始形状（工具与写入确认都挂在它上面）。 */
function blank(overrides) {
  return {
    id: '',
    role: 'assistant',
    content: '',
    createdAt: 0,
    streaming: false,
    error: '',
    tools: [],
    writeRequest: null,
    ...overrides,
  }
}

/** Rust 落盘的消息 → 界面用的消息。 */
export function storedToMessages(records) {
  return (records ?? []).map(record =>
    blank({
      id: record.id,
      role: record.role,
      content: record.content ?? '',
      createdAt: record.createdAt ?? 0,
      error: record.error ?? '',
    }),
  )
}

/** 提交时先插一条用户消息（乐观显示）。 */
export function appendUserMessage(messages, { id, text, createdAt = 0 }) {
  return [...messages, blank({ id, role: 'user', content: text, createdAt })]
}

/** 助手消息的占位。已经有了就原样返回。 */
export function ensureAssistantMessage(messages, id, { createdAt = 0 } = {}) {
  if (messages.some(message => message.id === id)) return messages
  return [...messages, blank({ id, createdAt, streaming: true })]
}

/** 追加一段增量文本。 */
export function applyDelta(messages, id, text, { createdAt = 0 } = {}) {
  const index = messages.findIndex(message => message.id === id)
  if (index < 0) {
    // 事件比命令返回值先到：直接补一条助手消息。它一定排在用户消息之后，
    // 因为用户消息是提交那一刻同步插进列表的。
    return [...messages, blank({ id, content: text, createdAt, streaming: true })]
  }
  const next = messages.slice()
  next[index] = { ...next[index], content: next[index].content + text, streaming: true }
  return next
}

/** 流结束（正常说完，或带着原因被打断）。 */
export function finishStream(messages, id, error = '') {
  const index = messages.findIndex(message => message.id === id)
  if (index < 0) return messages
  const next = messages.slice()
  next[index] = { ...next[index], streaming: false, error }
  return next
}

/** 发送失败时把乐观插入的消息撤掉：界面上不该留下历史里没有的消息。 */
export function removeMessage(messages, id) {
  return messages.filter(message => message.id !== id)
}

// ---------- 工具卡片 ----------

/**
 * 一条消息的某个字段变了：替换成新对象，没这条消息就补一条助手消息。
 * 工具事件、写入请求都用它，省得每处都写一遍「找不到怎么办」。
 */
function patchMessage(messages, messageId, patch) {
  const index = messages.findIndex(message => message.id === messageId)
  if (index < 0) {
    return [...messages, blank({ id: messageId, createdAt: Date.now(), streaming: true, ...patch })]
  }
  const next = messages.slice()
  next[index] = { ...next[index], ...patch }
  return next
}

/** 工具开始跑了：插一张「正在读取 xxx」的卡片。 */
export function startToolCall(messages, messageId, { callId, name, label, args = null }) {
  const index = messages.findIndex(message => message.id === messageId)
  const current = index < 0 ? [] : messages[index].tools ?? []
  // 同一个 callId 重复到达（重连、事件重发）时不插第二张卡。
  if (current.some(tool => tool.callId === callId)) return messages
  return patchMessage(messages, messageId, {
    tools: [...current, { callId, name, label, args, status: 'running', summary: '' }],
  })
}

/** 工具跑完了：更新卡片的状态与那句短评。找不到卡片时补一张，免得结果凭空消失。 */
export function finishToolCall(messages, messageId, { callId, name, ok, summary }) {
  const index = messages.findIndex(message => message.id === messageId)
  const tools = index < 0 ? [] : messages[index].tools ?? []
  const found = tools.some(tool => tool.callId === callId)
  const next = found
    ? tools.map(tool =>
        tool.callId === callId ? { ...tool, status: ok ? 'ok' : 'failed', summary } : tool,
      )
    : [...tools, { callId, name, label: summary || name, args: null, status: ok ? 'ok' : 'failed', summary }]
  return patchMessage(messages, messageId, { tools: next })
}

/** 收到写入确认：把 diff 挂到这条消息上，等用户按按钮。 */
export function attachWriteRequest(messages, messageId, { requestId, path, diff, reason = '' }) {
  return patchMessage(messages, messageId, {
    writeRequest: { requestId, path, diff, reason, state: 'pending', message: '' },
  })
}

/** 用户按了「应用」/「拒绝」，或者这一步作废了。 */
export function resolveWriteRequest(messages, messageId, requestId, { state, message = '' }) {
  const index = messages.findIndex(message => message.id === messageId)
  if (index < 0) return messages
  const request = messages[index].writeRequest
  if (!request || request.requestId !== requestId) return messages
  const next = messages.slice()
  next[index] = { ...next[index], writeRequest: { ...request, state, message } }
  return next
}

/**
 * 这条消息的流结束了，还挂着的写入确认就永远等不到回答了（Agent 循环已经被取消）。
 * 把它标成「已作废」，而不是让按钮一直亮着骗用户点。
 */
export function expireWriteRequests(messages, messageId) {
  const index = messages.findIndex(message => message.id === messageId)
  if (index < 0) return messages
  const request = messages[index].writeRequest
  if (!request || request.state !== 'pending') return messages
  const next = messages.slice()
  next[index] = {
    ...next[index],
    writeRequest: { ...request, state: 'expired', message: '这一步已经结束了，没有改动。' },
  }
  return next
}

/** 把 diff 原文按行拆开并分类，交给卡片上色。 */
export function diffLines(diff) {
  return (diff ?? '').split('\n').map((text, index) => ({ index, text, kind: lineKind(text) }))
}

function lineKind(text) {
  if (text.startsWith('+++') || text.startsWith('---')) return 'file'
  if (text.startsWith('@@')) return 'hunk'
  if (text.startsWith('+')) return 'added'
  if (text.startsWith('-')) return 'removed'
  if (text.startsWith('\\')) return 'note'
  if (text.trim() === '') return 'blank'
  return 'context'
}
