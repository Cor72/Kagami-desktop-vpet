import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

export const PET_EXPRESSIONS = ['smile', 'squint', 'tears', 'teardrop']

// Vue → Rust：参数对象的 name 对应 Rust 命令的 name 参数。
export function requestExpression(name) {
  return invoke('request_expression', { name })
}

// Rust → Vue：订阅成功后，返回取消订阅的函数。
export function onExpressionRequested(handler) {
  return listen('pet-expression-requested', event => {
    handler(event.payload.name)
  })
}
