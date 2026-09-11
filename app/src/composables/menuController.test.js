import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createMenuController } from './menuController.js'

const compact = { width: 340, height: 440, side: 'right', modelOffsetX: 0 }
const expanded = { ...compact, width: 480 }
const deferred = () => { let resolve; const promise = new Promise(r => { resolve = r }); return { promise, resolve } }

test('快速打开再关闭会串行还原原生窗口，不显示过期菜单', async () => {
  const gate = deferred(), calls = [], changes = []
  const menu = createMenuController({
    setOpen: open => { calls.push(open); return open ? gate.promise : Promise.resolve(compact) },
    onChange: value => changes.push(value),
  })
  menu.dispatch('context')
  const finished = menu.dispatch('context')
  assert.deepEqual(calls, [true])
  gate.resolve(expanded)
  await finished
  assert.deepEqual(calls, [true, false])
  assert.equal(changes.at(-1).state, 'closed')
  assert.equal(changes.at(-1).layout.width, 340)
  assert.ok(changes.every(change => change.state === 'closed'))
})

test('切换二级不请求窗口调整，收起失败恢复可操作的菜单', async () => {
  const calls = [], errors = [], changes = []
  const menu = createMenuController({
    setOpen: async open => { calls.push(open); if (!open) throw Error('resize failed'); return expanded },
    onChange: value => changes.push(value), onError: cause => errors.push(cause),
  })
  await menu.dispatch('context')
  await menu.dispatch('expressions')
  assert.deepEqual(calls, [true])
  assert.equal(changes.at(-1).state, 'expressions')
  await menu.dispatch('close')
  assert.equal(changes.at(-1).state, 'expressions')
  assert.equal(errors.length, 1)
})

test('托盘原生重置后，迟到的展开响应不会恢复旧布局', async () => {
  const gate = deferred(), changes = [], calls = []
  let actualOpen = false
  const menu = createMenuController({
    setOpen: async open => {
      calls.push(open)
      if (open) await gate.promise
      actualOpen = open
      return open ? { ...expanded, side: 'left', modelOffsetX: 140 } : compact
    },
    onChange: value => changes.push(value),
  })
  const finished = menu.dispatch('context')
  menu.reset(compact)
  gate.resolve({ ...expanded, side: 'left', modelOffsetX: 140 })
  await finished
  assert.equal(changes.at(-1).state, 'closed')
  assert.equal(changes.at(-1).layout.modelOffsetX, 0)
  assert.equal(actualOpen, false)
  assert.deepEqual(calls, [true, false])
})

test('同一事件循环内二级再返回，不丢失最后一个状态', async () => {
  const changes = []
  const menu = createMenuController({ setOpen: async () => expanded, onChange: value => changes.push(value) })
  await menu.dispatch('context')
  menu.dispatch('expressions')
  await menu.dispatch('back')
  assert.equal(changes.at(-1).state, 'root')
})
