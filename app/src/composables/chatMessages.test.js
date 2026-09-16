import assert from 'node:assert/strict'
import test from 'node:test'
import {
  applyDelta, appendUserMessage, attachWriteRequest, diffLines, ensureAssistantMessage,
  expireWriteRequests, finishStream, finishToolCall, removeMessage, resolveWriteRequest,
  startToolCall, storedToMessages,
} from './chatMessages.js'

/** 一条助手占位消息：工具卡片与写入确认都挂在它上面。 */
function assistantPlaceholder() {
  return ensureAssistantMessage(appendUserMessage([], { id: 'u1', text: '看看工作区' }), 'a1')
}

test('把落盘的消息转成界面用的形状，缺字段有默认值', () => {
  const messages = storedToMessages([
    { id: 'm1', role: 'user', content: '在吗', createdAt: 100 },
    { id: 'm2', role: 'assistant', content: '在的', createdAt: 200, error: '已停止' },
    { id: 'm3', role: 'assistant' },
  ])
  assert.deepEqual(messages[0], {
    id: 'm1', role: 'user', content: '在吗', createdAt: 100, streaming: false, error: '',
    tools: [], writeRequest: null,
  })
  assert.equal(messages[1].error, '已停止')
  assert.equal(messages[2].content, '')
  assert.deepEqual(messages[2].tools, [], '历史消息没有工具卡片，但形状要一致')
  assert.deepEqual(storedToMessages(undefined), [])
})

test('增量按顺序拼起来，且不改动传进来的数组', () => {
  const start = appendUserMessage([], { id: 'u1', text: '你好' })
  const withPlaceholder = ensureAssistantMessage(start, 'a1')
  const first = applyDelta(withPlaceholder, 'a1', '你')
  const second = applyDelta(first, 'a1', '好')
  assert.equal(second.at(-1).content, '你好')
  assert.equal(withPlaceholder.at(-1).content, '', '原数组不该被改')
  assert.equal(first.at(-1).content, '你')
})

test('事件比命令返回值先到时会自己补一条助手消息，顺序仍在用户消息之后', () => {
  const messages = appendUserMessage([], { id: 'u1', text: '在吗' })
  const afterDelta = applyDelta(messages, 'a1', '在')
  assert.deepEqual(afterDelta.map(message => message.role), ['user', 'assistant'])
  // 命令返回值随后到达：不该重复插入。
  const afterEnsure = ensureAssistantMessage(afterDelta, 'a1')
  assert.equal(afterEnsure.length, 2)
  assert.equal(afterEnsure.at(-1).content, '在')
})

test('结束与失败都会关掉流式状态并带上原因', () => {
  const messages = ensureAssistantMessage(appendUserMessage([], { id: 'u1', text: 'hi' }), 'a1')
  const done = finishStream(messages, 'a1')
  assert.equal(done.at(-1).streaming, false)
  assert.equal(done.at(-1).error, '')

  const stopped = finishStream(messages, 'a1', '已停止')
  assert.equal(stopped.at(-1).streaming, false)
  assert.equal(stopped.at(-1).error, '已停止')
  // 找不到的消息不该出异常，也不该改变列表。
  assert.equal(finishStream(messages, 'missing').length, 2)
})

test('发送失败可以撤回乐观插入的消息', () => {
  const messages = appendUserMessage([], { id: 'local-1', text: '发不出去的话' })
  assert.deepEqual(removeMessage(messages, 'local-1'), [])
})

// ---------- 阶段 C：工具卡片 ----------

test('工具开始跑时插一张卡片，跑完就地更新，不会多出一张', () => {
  const started = startToolCall(assistantPlaceholder(), 'a1', {
    callId: 'call_1', name: 'read_file', label: '正在读取 pomodoro.rs',
  })
  const tool = started.at(-1).tools[0]
  assert.equal(tool.label, '正在读取 pomodoro.rs')
  assert.equal(tool.status, 'running')

  const done = finishToolCall(started, 'a1', {
    callId: 'call_1', name: 'read_file', ok: true, summary: '读过 pomodoro.rs 了',
  })
  assert.equal(done.at(-1).tools.length, 1)
  assert.equal(done.at(-1).tools[0].status, 'ok')
  assert.equal(done.at(-1).tools[0].summary, '读过 pomodoro.rs 了')

  // 同一个 callId 重复到达（事件重发）时不该插第二张。
  const again = startToolCall(done, 'a1', { callId: 'call_1', name: 'read_file', label: 'x' })
  assert.equal(again.at(-1).tools.length, 1)
})

test('工具失败时卡片进入失败态', () => {
  const started = startToolCall(assistantPlaceholder(), 'a1', {
    callId: 'call_2', name: 'list_files', label: '看看工作区里有什么',
  })
  const failed = finishToolCall(started, 'a1', {
    callId: 'call_2', name: 'list_files', ok: false, summary: '没看成',
  })
  assert.equal(failed.at(-1).tools[0].status, 'failed')
})

test('结果比开始先到（事件重发、顺序错乱）时补一张卡片，结果不会凭空消失', () => {
  const messages = finishToolCall(assistantPlaceholder(), 'a1', {
    callId: 'call_9', name: 'grep', ok: true, summary: '搜过「松饼」了',
  })
  assert.equal(messages.at(-1).tools.length, 1)
  assert.equal(messages.at(-1).tools[0].status, 'ok')
})

// ---------- 阶段 C：写入确认 ----------

test('写入确认挂到消息上，用户按下按钮后状态才变', () => {
  const withRequest = attachWriteRequest(assistantPlaceholder(), 'a1', {
    requestId: 'wr1', path: 'C:\\proj\\a.rs', diff: '-旧\n+新\n', reason: '把名字改短一点',
  })
  const request = withRequest.at(-1).writeRequest
  assert.equal(request.state, 'pending')
  assert.equal(request.reason, '把名字改短一点')

  const applied = resolveWriteRequest(withRequest, 'a1', 'wr1', { state: 'applied', message: '已写入' })
  assert.equal(applied.at(-1).writeRequest.state, 'applied')
  assert.equal(applied.at(-1).writeRequest.message, '已写入')
  assert.equal(withRequest.at(-1).writeRequest.state, 'pending', '原数组不该被改')
})

test('requestId 对不上时什么都不改（重复点击不该动到别的卡片）', () => {
  const withRequest = attachWriteRequest(assistantPlaceholder(), 'a1', {
    requestId: 'wr1', path: 'a.rs', diff: '',
  })
  const other = resolveWriteRequest(withRequest, 'a1', 'wr2', { state: 'applied' })
  assert.equal(other, withRequest)
})

test('流结束时还挂着的写入确认标成已作废，按钮不会再骗用户点', () => {
  const withRequest = attachWriteRequest(assistantPlaceholder(), 'a1', {
    requestId: 'wr1', path: 'a.rs', diff: '-a\n+b\n',
  })
  const expired = expireWriteRequests(withRequest, 'a1')
  assert.equal(expired.at(-1).writeRequest.state, 'expired')
  // 已经有结论的卡片不动。
  const applied = resolveWriteRequest(withRequest, 'a1', 'wr1', { state: 'applied' })
  assert.equal(expireWriteRequests(applied, 'a1').at(-1).writeRequest.state, 'applied')
})

test('diff 按行前缀分类，前端只负责上色', () => {
  const lines = diffLines('--- 修改前\n+++ 修改后\n@@ -1 +1 @@\n-旧的一行\n+新的一行\n 没变的\n\\ No newline at end of file\n')
  const kinds = lines.map(line => line.kind)
  assert.deepEqual(kinds, ['file', 'file', 'hunk', 'removed', 'added', 'context', 'note', 'blank'])
  assert.equal(lines[3].text, '-旧的一行')
  assert.deepEqual(diffLines(''), [{ index: 0, text: '', kind: 'blank' }])
})
