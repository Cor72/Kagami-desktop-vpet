import assert from 'node:assert/strict'
import test from 'node:test'
import { applyDelta, appendUserMessage, ensureAssistantMessage, finishStream, removeMessage, storedToMessages } from './chatMessages.js'

test('把落盘的消息转成界面用的形状，缺字段有默认值', () => {
  const messages = storedToMessages([
    { id: 'm1', role: 'user', content: '在吗', createdAt: 100 },
    { id: 'm2', role: 'assistant', content: '在的', createdAt: 200, error: '已停止' },
    { id: 'm3', role: 'assistant' },
  ])
  assert.deepEqual(messages[0], { id: 'm1', role: 'user', content: '在吗', createdAt: 100, streaming: false, error: '' })
  assert.equal(messages[1].error, '已停止')
  assert.equal(messages[2].content, '')
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
