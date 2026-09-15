// 消息列表的纯函数操作。
//
// 单独抽出来的两个理由：
// 1. 流式拼接是最容易出错的逻辑，写成纯函数就能用 `node --test` 直接覆盖；
// 2. 事件与命令返回值**谁先到都有可能**——`chat-stream-delta` 可能比
//    `send_message` 的返回值先到。这里用「按 id 找，找不到就补一条」统一处理，
//    调用方不需要关心时序。

/** Rust 落盘的消息 → 界面用的消息。 */
export function storedToMessages(records) {
  return (records ?? []).map(record => ({
    id: record.id,
    role: record.role,
    content: record.content ?? '',
    createdAt: record.createdAt ?? 0,
    streaming: false,
    error: record.error ?? '',
  }))
}

/** 提交时先插一条用户消息（乐观显示）。 */
export function appendUserMessage(messages, { id, text, createdAt = 0 }) {
  return [...messages, { id, role: 'user', content: text, createdAt, streaming: false, error: '' }]
}

/** 助手消息的占位。已经有了就原样返回。 */
export function ensureAssistantMessage(messages, id, { createdAt = 0 } = {}) {
  if (messages.some(message => message.id === id)) return messages
  return [...messages, { id, role: 'assistant', content: '', createdAt, streaming: true, error: '' }]
}

/** 追加一段增量文本。 */
export function applyDelta(messages, id, text, { createdAt = 0 } = {}) {
  const index = messages.findIndex(message => message.id === id)
  if (index < 0) {
    // 事件比命令返回值先到：直接补一条助手消息。它一定排在用户消息之后，
    // 因为用户消息是提交那一刻同步插进列表的。
    return [
      ...messages,
      { id, role: 'assistant', content: text, createdAt, streaming: true, error: '' },
    ]
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
