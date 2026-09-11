import assert from 'node:assert/strict'
import test from 'node:test'
import { createPetGesture } from './petGesture.js'

const pointer = (extra = {}) => ({ pointerId: 1, button: 0, buttons: 1, clientX: 100, clientY: 100, ...extra })

test('短按在松开时切换表情，微小抖动不启动拖动', () => {
  const gesture = createPetGesture()
  assert.equal(gesture.pointerDown(pointer()), 'none')
  assert.equal(gesture.pointerMove(pointer({ clientX: 103, clientY: 103 })), 'none')
  assert.equal(gesture.pointerUp(pointer({ buttons: 0, clientX: 103 })), 'click')
  assert.equal(gesture.pointerUp(pointer({ buttons: 0 })), 'none')
})

test('达到六像素才开始拖动，每次手势仅启动一次，松开不切表情', () => {
  const gesture = createPetGesture()
  gesture.pointerDown(pointer())
  assert.equal(gesture.pointerMove(pointer({ clientX: 106 })), 'drag')
  assert.equal(gesture.pointerMove(pointer({ clientX: 120 })), 'none')
  assert.equal(gesture.pointerUp(pointer({ clientX: 120, buttons: 0 })), 'none')
})

test('鼠标松开、取消或不同指针不会遗留拖动或误触单击', () => {
  const gesture = createPetGesture()
  gesture.pointerDown(pointer())
  assert.equal(gesture.pointerMove(pointer({ pointerId: 2, clientX: 150 })), 'none')
  assert.equal(gesture.pointerMove(pointer({ buttons: 0, clientX: 150 })), 'none')
  assert.equal(gesture.pointerUp(pointer({ buttons: 0 })), 'none')
  gesture.pointerDown(pointer())
  gesture.cancel()
  assert.equal(gesture.pointerUp(pointer({ buttons: 0 })), 'none')
})

test('右键和远距离松开不会当成左键单击', () => {
  const gesture = createPetGesture()
  gesture.pointerDown(pointer({ button: 2, buttons: 2 }))
  assert.equal(gesture.pointerUp(pointer({ button: 2, buttons: 0 })), 'none')
  gesture.pointerDown(pointer())
  assert.equal(gesture.pointerUp(pointer({ clientX: 150, buttons: 0 })), 'none')
})
