import assert from 'node:assert/strict'
import test from 'node:test'
import { createMouseFollow, getFocusTarget } from './mouseFollow.js'

const bounds = { left: 8, top: 12, width: 320, height: 400 }

test('鼠标以画布中心为原点，左右与上下方向正确，扣除页面留白', () => {
  for (const [pointer, expected] of [
    [{ x: 168, y: 212 }, { x: 0, y: 0 }],
    [{ x: 248, y: 112 }, { x: 0.5, y: 0.5 }],
    [{ x: 88, y: 312 }, { x: -0.5, y: -0.5 }],
  ]) {
    assert.deepEqual(getFocusTarget(pointer, bounds), expected)
  }
})

test('窗口外和其他屏幕上的鼠标仍可跟随，远距离与对角线幅度有界', () => {
  assert.deepEqual(getFocusTarget({ x: -4000, y: 212 }, bounds), { x: -1, y: 0 })
  assert.deepEqual(getFocusTarget({ x: 168, y: -2000 }, bounds), { x: 0, y: 1 })
  const diagonal = getFocusTarget({ x: 1768, y: -1788 }, bounds)
  assert.ok(diagonal.x > 0 && diagonal.y > 0)
  assert.ok(Math.abs(Math.hypot(diagonal.x, diagonal.y) - 1) < 1e-10)
})

test('画布无尺寸或坐标无效时回正，不把 NaN 交给模型', () => {
  for (const rect of [bounds, { ...bounds, width: 0 }, { ...bounds, height: 0 }]) {
    assert.deepEqual(getFocusTarget({ x: NaN, y: 3 }, rect), { x: 0, y: 0 })
  }
  assert.deepEqual(getFocusTarget({ x: 300, y: 2 }, { ...bounds, width: 0 }), { x: 0, y: 0 })
})

function deferred() {
  let resolve, reject
  const promise = new Promise((yes, no) => { resolve = yes; reject = no })
  return { promise, resolve, reject }
}

function setup(readPointer) {
  const targets = []
  const errors = []
  let currentBounds = bounds
  let now = 0
  const follow = createMouseFollow({
    readPointer,
    getBounds: () => currentBounds,
    setFocus: (x, y) => targets.push({ x, y }),
    onError: error => errors.push(error),
    now: () => now,
  })
  return {
    follow, targets, errors,
    resize: rect => { currentBounds = rect },
    advance: ms => { now += ms },
  }
}

test('仅运行时采样，同一时刻最多一条请求，结果写入跟随目标', async () => {
  const sample = deferred()
  let reads = 0
  const { follow, targets } = setup(() => { reads++; return sample.promise })
  await follow.update()
  assert.equal(reads, 0)
  follow.setRunning(true)
  const pending = follow.update()
  await follow.update()
  assert.equal(reads, 1)
  sample.resolve({ x: 248, y: 112 })
  await pending
  assert.deepEqual(targets.at(-1), { x: 0.5, y: 0.5 })
  follow.setRunning(false)
  await follow.update()
  assert.equal(reads, 1)
})

test('隐藏后立即恢复也会丢弃隐藏前的请求，下一帧重新读取位置', async () => {
  const sample = deferred()
  let reads = 0
  const { follow, targets } = setup(() => ++reads === 1 ? sample.promise : Promise.resolve({ x: 8, y: 212 }))
  follow.setRunning(true)
  const pending = follow.update()
  follow.setRunning(false)
  follow.setRunning(true)
  const count = targets.length
  sample.resolve({ x: 328, y: 212 })
  await pending
  assert.equal(targets.length, count, '旧位置不应写回模型')
  await follow.update()
  assert.deepEqual(targets.at(-1), { x: -1, y: 0 })
})

test('模型卸载后不再采样，未完成的请求不会访问已销毁模型', async () => {
  const sample = deferred()
  let reads = 0
  const { follow, targets } = setup(() => { reads++; return sample.promise })
  follow.setRunning(true)
  const pending = follow.update()
  follow.destroy()
  const count = targets.length
  sample.resolve({ x: 328, y: 212 })
  await pending
  follow.setRunning(true)
  await follow.update()
  assert.equal(reads, 1)
  assert.equal(targets.length, count)
})

test('窗口布局变化后按新的画布位置换算，不缓存旧尺寸', async () => {
  const { follow, targets, resize } = setup(async () => ({ x: 168, y: 212 }))
  follow.setRunning(true)
  await follow.update()
  assert.deepEqual(targets.at(-1), { x: 0, y: 0 })
  resize({ left: 8, top: 12, width: 640, height: 400 })
  await follow.update()
  assert.deepEqual(targets.at(-1), { x: -0.5, y: 0 })
})

test('读取失败时回正并限速重试，同一轮故障只报告一次，成功后恢复跟随', async () => {
  let reads = 0
  const failure = new Error('读取鼠标失败')
  const { follow, targets, errors, advance } = setup(async () => {
    if (++reads < 3) throw failure
    return { x: 328, y: 212 }
  })
  follow.setRunning(true)
  await follow.update()
  assert.deepEqual(targets.at(-1), { x: 0, y: 0 })
  assert.deepEqual(errors, [failure])
  await follow.update()
  assert.equal(reads, 1)
  advance(1000)
  await follow.update()
  assert.equal(reads, 2)
  assert.equal(errors.length, 1)
  advance(1000)
  await follow.update()
  assert.deepEqual(targets.at(-1), { x: 1, y: 0 })
})
