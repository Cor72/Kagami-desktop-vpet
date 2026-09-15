// 把高频的流式增量合批刷新。
//
// 为什么必须有这一层（计划 §10 第 3 条）：模型每吐几个字就发一条 `chat-stream-delta`，
// 每个事件都改一次 DOM 会把主线程排满，直接抢走 Live2D 的帧预算——桌宠会肉眼可见地卡。
// 这里按约 50ms 合一次，观感上仍然是「逐字蹦出来」，但渲染次数降到十分之一。
//
// 定时器由调用方注入，测试里可以换成假的，不用真的等 50ms。

/** 刷新 DOM 的最小间隔（毫秒）。 */
export const STREAM_THROTTLE_MS = 50

export function createDeltaBatch({
  onFlush,
  delay = STREAM_THROTTLE_MS,
  // 包一层而不是直接把 setTimeout 传进去：解构出来的定时器在某些浏览器里
  // 会因为丢了 this 报 Illegal invocation。
  schedule = (callback, wait) => setTimeout(callback, wait),
  cancel = id => clearTimeout(id),
}) {
  let pending = new Map()
  let timer = null

  /** 立刻把攒下的增量交出去（流结束、失败、停止时都要先调它，否则会丢尾巴）。 */
  function flush() {
    if (timer !== null) {
      cancel(timer)
      timer = null
    }
    if (!pending.size) return
    const batch = [...pending]
    pending = new Map()
    onFlush(batch)
  }

  /** 记一段增量。同一批里的文本按消息 id 合并。 */
  function push(id, text) {
    if (!text) return
    pending.set(id, (pending.get(id) ?? '') + text)
    // 已经有定时器在等：这段文本会跟着同一批走，不再单独排一次。
    if (timer === null) timer = schedule(flush, delay)
  }

  /** 组件卸载时调用：丢掉没来得及刷的增量，别再触发渲染。 */
  function dispose() {
    if (timer !== null) {
      cancel(timer)
      timer = null
    }
    pending = new Map()
  }

  return { push, flush, dispose, pendingCount: () => pending.size }
}
