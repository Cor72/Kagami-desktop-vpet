import assert from 'node:assert/strict'
import test from 'node:test'
import { STREAM_THROTTLE_MS, createDeltaBatch } from './deltaBatch.js'

/** 假定时器：手动决定什么时候触发，测试不用真的等。 */
function fakeScheduler() {
  let nextId = 0
  const timers = new Map()
  return {
    schedule(callback, delay) {
      const id = ++nextId
      timers.set(id, { callback, delay })
      return id
    },
    cancel(id) {
      timers.delete(id)
    },
    pending() {
      return [...timers.entries()]
    },
    runAll() {
      const entries = [...timers.entries()]
      timers.clear()
      for (const [, timer] of entries) timer.callback()
    },
  }
}

function batchOf() {
  const flushed = []
  const timer = fakeScheduler()
  const batch = createDeltaBatch({
    onFlush: entries => flushed.push(entries),
    schedule: timer.schedule,
    cancel: timer.cancel,
  })
  return { batch, flushed, timer }
}

test('节流间隔是计划要求的 50 毫秒', () => {
  assert.equal(STREAM_THROTTLE_MS, 50)
  const { batch, timer } = batchOf()
  batch.push('a1', '你')
  assert.deepEqual(timer.pending().map(([, t]) => t.delay), [50])
})

test('同一批里的多次增量只刷一次，文本合并', () => {
  const { batch, flushed, timer } = batchOf()
  batch.push('a1', '你')
  batch.push('a1', '好')
  batch.push('a2', '呀')
  assert.equal(timer.pending().length, 1, '重复 push 不该排第二个定时器')
  timer.runAll()
  assert.deepEqual(flushed, [[['a1', '你好'], ['a2', '呀']]])
})

test('刷完之后再来增量会重新排一次', () => {
  const { batch, flushed, timer } = batchOf()
  batch.push('a1', '一')
  timer.runAll()
  batch.push('a1', '二')
  assert.equal(timer.pending().length, 1)
  timer.runAll()
  assert.deepEqual(flushed, [[['a1', '一']], [['a1', '二']]])
})

test('主动 flush 会取消已排的定时器，不重复刷新也不丢文本', () => {
  const { batch, flushed, timer } = batchOf()
  batch.push('a1', '最后一句')
  batch.flush()
  assert.equal(timer.pending().length, 0)
  timer.runAll()
  assert.deepEqual(flushed, [[['a1', '最后一句']]])
})

test('空文本不排定时器，dispose 之后不再刷新', () => {
  const { batch, flushed, timer } = batchOf()
  batch.push('a1', '')
  assert.equal(timer.pending().length, 0)
  batch.push('a1', '半句')
  assert.equal(batch.pendingCount(), 1)
  batch.dispose()
  assert.equal(timer.pending().length, 0)
  assert.equal(batch.pendingCount(), 0)
  timer.runAll()
  assert.deepEqual(flushed, [])
})
