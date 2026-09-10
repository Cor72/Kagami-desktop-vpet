import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'

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

export const getPetSettings = () => invoke('get_pet_settings')
export const setPetVisible = visible => invoke('set_pet_visible', { visible })
// Rust 的 max_fps 参数在 JS 调用中使用 camelCase：maxFps。
export const setPetMaxFps = maxFps => invoke('set_pet_max_fps', { maxFps })
export const setPetAlwaysOnTop = enabled => invoke('set_pet_always_on_top', { enabled })
export const quitPet = () => invoke('quit_pet')
export const startPetDragging = () => getCurrentWindow().startDragging()

export const onPetSettingsChanged = handler =>
  listen('pet-settings-changed', event => handler(event.payload))

export const onPetDesktopError = handler =>
  listen('pet-desktop-error', event => handler(event.payload))
