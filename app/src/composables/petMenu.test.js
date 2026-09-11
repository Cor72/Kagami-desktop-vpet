import assert from 'node:assert/strict'
import test from 'node:test'
import { getMenuItems, pickExpression, transitionMenu } from './petMenu.js'

test('随机表情排除上次实际表情，同时能选到其余每一种', () => {
  const names = ['smile', 'squint', 'tears', 'teardrop']
  assert.equal(pickExpression(names, 'smile', () => 0), 'squint')
  assert.equal(pickExpression(names, 'smile', () => 0.5), 'tears')
  assert.equal(pickExpression(names, 'smile', () => 0.999), 'teardrop')
  assert.equal(pickExpression(['smile'], 'smile'), 'smile')
  assert.equal(pickExpression([], ''), null)
})

test('表情二级完全替换一级五个入口，返回在第五个位置', () => {
  let state = transitionMenu('closed', 'context')
  assert.equal(state, 'root')
  assert.deepEqual(getMenuItems(state).map(item => item.id), ['expressions', 'settings', 'always-on-top', 'hide', 'quit'])
  state = transitionMenu(state, 'expressions')
  assert.deepEqual(getMenuItems(state).map(item => item.id), ['smile', 'squint', 'tears', 'teardrop', 'back'])
  state = transitionMenu(state, 'back')
  assert.equal(state, 'root')
})

test('Esc逐层返回，右键关闭所有层，设置不进入二级状态', () => {
  assert.equal(transitionMenu('expressions', 'escape'), 'root')
  assert.equal(transitionMenu('root', 'escape'), 'closed')
  assert.equal(transitionMenu('expressions', 'context'), 'closed')
  assert.equal(transitionMenu('root', 'settings'), 'root')
  assert.equal(transitionMenu('expressions', 'close'), 'closed')
  assert.deepEqual(getMenuItems('closed'), [])
})
