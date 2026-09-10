import assert from 'node:assert/strict'
import test from 'node:test'
import { selectRenderPolicy } from './renderPolicy.js'

test('主动隐藏时停止更新，保留用户帧率', () => {
  assert.deepEqual(selectRenderPolicy(false, true, 30), { running: false, maxFps: 30 })
})
test('页面不可见或原生窗口最小化时停止', () => {
  assert.deepEqual(selectRenderPolicy(true, false, 15), { running: false, maxFps: 15 })
  assert.deepEqual(selectRenderPolicy(true, true, 30, true), { running: false, maxFps: 30 })
})
test('恢复显示后继续使用 15 FPS；失焦不参与暂停条件', () => {
  assert.deepEqual(selectRenderPolicy(true, true, 15), { running: true, maxFps: 15 })
})
