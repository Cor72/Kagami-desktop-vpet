import assert from 'node:assert/strict'
import test from 'node:test'
import { AGENT_MODE, CHAT_MODE, DEFAULT_PROVIDER, PROVIDERS, isCustomProvider, latestAiConfig, providerPreset } from './aiConfig.js'

// 这些默认值要和 Rust 的 `ai/config.rs` 对得上，否则设置窗口里显示的服务商会
// 与真正发请求的那家不一致——用户会觉得「我明明选的是 DeepSeek」。
test('默认服务商是 DeepSeek，地址与模型与 Rust 侧一致', () => {
  assert.equal(DEFAULT_PROVIDER, 'deepseek')
  const deepseek = providerPreset(DEFAULT_PROVIDER)
  assert.equal(deepseek.baseUrl, 'https://api.deepseek.com/v1')
  assert.equal(deepseek.model, 'deepseek-chat')
})

test('三家预设都在，id 不重复，自定义没有默认地址', () => {
  assert.deepEqual(PROVIDERS.map(provider => provider.id), ['deepseek', 'openai', 'custom'])
  assert.equal(new Set(PROVIDERS.map(provider => provider.id)).size, PROVIDERS.length)
  assert.equal(providerPreset('custom').baseUrl, '')
  assert.equal(providerPreset('custom').model, '')
  assert.equal(isCustomProvider('custom'), true)
  assert.equal(isCustomProvider('deepseek'), false)
})

test('认不出来的服务商回退到第一个预设，不会崩', () => {
  assert.equal(providerPreset('gemini').id, 'deepseek')
  assert.equal(providerPreset(undefined).id, 'deepseek')
})

test('模式常量与 Rust 的两个取值一致', () => {
  assert.equal(CHAT_MODE, 'chat')
  assert.equal(AGENT_MODE, 'agent')
})

test('旧版本的快照不会覆盖新的', () => {
  const current = { revision: 5, model: 'new' }
  assert.equal(latestAiConfig(current, { revision: 4, model: 'old' }), current)
  assert.equal(latestAiConfig(current, { revision: 5, model: 'same' }).model, 'same')
  assert.equal(latestAiConfig(null, { revision: 1 }).revision, 1)
})
