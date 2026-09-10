import assert from 'node:assert/strict'
import test from 'node:test'
import { latestSettings } from './petSettings.js'

test('先收到托盘事件，再收到旧的初始化快照，不回退设置', () => {
  const event = { revision: 2, visible: false, maxFps: 15, alwaysOnTop: true }
  const initial = { revision: 0, visible: true, maxFps: 30, alwaysOnTop: true }
  assert.deepEqual(latestSettings(event, initial), event)
})

test('命令返回晚于更新事件时保留更新值，同版本和首次快照可接受', () => {
  const newer = { revision: 4, visible: true, maxFps: 15, alwaysOnTop: false }
  const older = { revision: 3, visible: true, maxFps: 15, alwaysOnTop: true }
  assert.deepEqual(latestSettings(newer, older), newer)
  assert.deepEqual(latestSettings(null, older), older)
  assert.deepEqual(latestSettings(older, newer), newer)
  assert.deepEqual(latestSettings(newer, { ...newer }), newer)
})
