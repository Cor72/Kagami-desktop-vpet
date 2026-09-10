import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'
import vm from 'node:vm'

async function createClock() {
  const source = await readFile(new URL(import.meta.resolve('easy-live2d')), 'utf8')
  const start = source.indexOf('var TimeManager = class')
  const end = source.indexOf('var Live2DContext = class', start)
  assert.ok(start >= 0 && end > start, '依赖时钟结构改变，需要复核补丁')
  let now = 1000000
  const TimeManager = vm.runInNewContext(`${source.slice(start, end)}; TimeManager`, {
    performance: { now: () => now }, Date: { now: () => now },
  })
  return { clock: new TimeManager(), advance: ms => { now += ms } }
}

test('首帧与暂停五分钟后恢复的首帧，不累计停顿时间', async () => {
  const { clock, advance } = await createClock()
  clock.update()
  assert.equal(clock.deltaTime, 0)
  advance(40)
  clock.update()
  assert.equal(clock.deltaTime, 0.04)
  advance(300000)
  clock.reset()
  clock.update()
  assert.equal(clock.deltaTime, 0)
  advance(67)
  clock.update()
  assert.equal(clock.deltaTime, 0.067)
})

test('异常长帧最多推进 100ms，时间回退不产生负步长', async () => {
  const { clock, advance } = await createClock()
  clock.update()
  advance(3000)
  clock.update()
  assert.equal(clock.deltaTime, 0.1)
  advance(-100)
  clock.update()
  assert.equal(clock.deltaTime, 0)
})
