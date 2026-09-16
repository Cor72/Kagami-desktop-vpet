// 服务商预设与配置快照的纯逻辑（不 import Tauri，可以直接用 node --test 覆盖）。
//
// **这里的默认值必须与 Rust 侧 `src-tauri/src/ai/config.rs` 一致**：
// 前端用它来渲染下拉框与输入框的初值，Rust 用它来真正发请求。两边各有一份测试钉着。

export const PROVIDERS = [
  {
    id: 'deepseek',
    label: 'DeepSeek',
    baseUrl: 'https://api.deepseek.com/v1',
    model: 'deepseek-chat',
  },
  {
    id: 'openai',
    label: 'OpenAI',
    baseUrl: 'https://api.openai.com/v1',
    model: 'gpt-4o-mini',
  },
  {
    // 自定义没有默认值：地址要用户自己填（Rust 侧同样留空）。
    id: 'custom',
    label: '自定义',
    baseUrl: '',
    model: '',
  },
]

export const DEFAULT_PROVIDER = 'deepseek'

/** 对话模式。与 Rust `ai::config::Mode` 的两个取值一致。 */
export const CHAT_MODE = 'chat'
export const AGENT_MODE = 'agent'

export function providerPreset(id) {
  return PROVIDERS.find(provider => provider.id === id) ?? PROVIDERS[0]
}

export function isCustomProvider(id) {
  return id === 'custom'
}

/** 只接受不比当前更新的快照，避免初始化时用旧值覆盖新事件（与 petSettings 同一套）。 */
export function latestAiConfig(current, incoming) {
  if (current && incoming.revision < current.revision) return current
  return incoming
}
