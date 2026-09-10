export function latestSettings(current, incoming) {
  // 托盘事件可能先于初始化快照或命令返回到达。
  if (current && incoming.revision < current.revision) return current
  return incoming
}
