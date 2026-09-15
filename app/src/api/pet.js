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
// 相对当前窗口客户区的逻辑坐标；鼠标在窗口外时也有效。
export const getPetCursorPosition = () => invoke('get_pet_cursor_position')
export const setPetVisible = visible => invoke('set_pet_visible', { visible })
// Rust 的 max_fps 参数在 JS 调用中使用 camelCase：maxFps。
export const setPetMaxFps = maxFps => invoke('set_pet_max_fps', { maxFps })
export const setPetAlwaysOnTop = enabled => invoke('set_pet_always_on_top', { enabled })
export const quitPet = () => invoke('quit_pet')
export const startPetDragging = () => getCurrentWindow().startDragging()
export const openPetSettings = () => invoke('open_pet_settings')
export const reloadPetModel = () => invoke('reload_pet_model')
export const onPetModelReload = handler => listen('pet-model-reload', handler)
export const onExpressionObserved = handler =>
  listen('pet-expression-observed', event => handler(event.payload.name))
export const isPetMinimized = () => getCurrentWindow().isMinimized()
export const onPetMinimized = handler =>
  listen('pet-window-minimized', event => handler(event.payload))

export const onPetSettingsChanged = handler =>
  listen('pet-settings-changed', event => handler(event.payload))

export const onPetDesktopError = handler =>
  listen('pet-desktop-error', event => handler(event.payload))

// ---------- 主动互动 ----------
// 八千代自己开口的那一套。文案全在 Rust 侧的内置模板里，这一层只负责收发。

export const getProactiveState = () => invoke('get_proactive_state')
// 走的是 pet-settings 那套更新路径，所以返回的是完整设置快照。
export const setProactiveEnabled = enabled => invoke('set_proactive_enabled', { enabled })
// 气泡消失时回报一句：acknowledged = 用户点了它（没点就算「被忽略」一次）。
export const proactiveDismiss = (id, acknowledged) =>
  invoke('proactive_dismiss', { id, acknowledged })
export const onProactiveSpeak = handler =>
  listen('proactive-speak', event => handler(event.payload))
