import assert from 'node:assert/strict'
import { describe, it } from 'node:test'
import { SPEECH_MAX_CHARS, clampSpeechText, speechAllowed } from './speechBubble.js'

describe('clampSpeechText', () => {
  it('keeps short lines as they are', () => {
    assert.equal(clampSpeechText('编辑器打开了：pomodoro.rs'), '编辑器打开了：pomodoro.rs')
  })

  it('flattens newlines and runs of whitespace', () => {
    assert.equal(clampSpeechText('  编辑器\n\n亮了   '), '编辑器 亮了')
  })

  it('truncates with an ellipsis at the limit', () => {
    const long = '一'.repeat(80)
    const clamped = clampSpeechText(long)
    assert.equal([...clamped].length, SPEECH_MAX_CHARS)
    assert.ok(clamped.endsWith('…'))
    assert.equal(clamped, `${'一'.repeat(SPEECH_MAX_CHARS - 1)}…`)
  })

  it('keeps text that is exactly at the limit untouched', () => {
    const exact = '字'.repeat(SPEECH_MAX_CHARS)
    assert.equal(clampSpeechText(exact), exact)
    assert.ok(!clampSpeechText(exact).includes('…'))
  })

  it('counts by code point so emoji and CJK are never cut in half', () => {
    const emoji = '👍'.repeat(50)
    const clamped = clampSpeechText(emoji)
    assert.equal([...clamped].length, SPEECH_MAX_CHARS)
    assert.ok(clamped.startsWith('👍'))
    assert.ok(clamped.endsWith('…'))
  })

  it('survives empty and missing input', () => {
    assert.equal(clampSpeechText(''), '')
    assert.equal(clampSpeechText(null), '')
    assert.equal(clampSpeechText(undefined), '')
  })
})

describe('speechAllowed', () => {
  it('only shows the bubble when the menu is closed', () => {
    assert.equal(speechAllowed('closed'), true)
    assert.equal(speechAllowed('root'), false)
    assert.equal(speechAllowed('expressions'), false)
  })
})
