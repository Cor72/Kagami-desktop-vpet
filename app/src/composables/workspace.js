// 工作区条目的纯逻辑（不 import Tauri，可以直接用 node --test 覆盖）。
//
// **这里的判断必须与 Rust 侧 `workspace.rs` 一致**：目录条目可读写、文件条目永远只读。
// 界面上画一把锁只是提示，真正的拒绝发生在 Rust 里（前端改了状态也拿不到写权限）。

/** 与 Rust `EntryKind` 的 serde 命名一致（`#[serde(rename_all = "lowercase")]`）。 */
export const DIR_ENTRY = 'dir'
export const FILE_ENTRY = 'file'

/** 拖进来的文件条目永远只读——它们只提供一条读取通道，物理上没有写入通道。 */
export function isReadOnly(entry) {
  return entry?.kind !== DIR_ENTRY
}

/** 卡片右侧那句权限说明。 */
export function accessLabel(entry) {
  return isReadOnly(entry) ? '只读' : '可读写'
}

/** 条目的类型名。 */
export function kindLabel(entry) {
  return isReadOnly(entry) ? '文件' : '文件夹'
}

/** 图标名（组件再映射到 @lucide/vue）。 */
export function kindIcon(entry) {
  return isReadOnly(entry) ? 'file' : 'folder'
}

/** 「N 条」这句提示；空的时候说清楚下一步该做什么。 */
export function countLabel(entries) {
  const list = entries ?? []
  return list.length === 0 ? '还没有授权任何文件' : `已授权 ${list.length} 条`
}

/**
 * 事件与命令返回值都可能带来一份新的条目清单（谁先到都有可能）。
 * 只接受数组；其它一律忽略，免得把界面弄成 undefined。
 */
export function acceptEntries(current, incoming) {
  if (!Array.isArray(incoming)) return current
  return incoming
}

/** 拖进来的东西里有没有「单个文件以外的」东西。 */
export function pickFilesOnly(paths) {
  return (paths ?? []).filter(path => typeof path === 'string' && path.trim() !== '')
}
