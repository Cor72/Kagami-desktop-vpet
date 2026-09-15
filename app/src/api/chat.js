// 对话窗口的 IPC 入口：命令名与事件名只写在这里，组件里不出现裸字符串。
// 约定与 `api/pet.js` 一致：Vue → Rust 用 invoke，参数对象用 camelCase；
// Rust → Vue 用 listen，订阅函数返回「取消订阅」的函数。
import { invoke } from '@tauri-apps/api/core'

export const CHAT_WINDOW_LABEL = 'chat'
export const CHAT_MODE = 'chat'
export const AGENT_MODE = 'agent'

// 右键桌宠 → 气泡菜单「对话」：打开（或聚焦）对话窗口。
// 窗口是单例，重复调用只会把已经存在的那个叫到前面来。
export const openChatWindow = () => invoke('open_chat_window')
