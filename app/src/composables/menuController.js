import { transitionMenu } from './petMenu.js'

export function createMenuController({ setOpen, onChange, onError = console.error }) {
  let state = 'closed'
  let wanted = 'closed'
  let layout = { width: 340, height: 440, side: 'right', modelOffsetX: 0 }
  let nativeOpen = false
  let resetVersion = 0
  let needsSync = false
  let running = null
  const publish = busy => onChange({ state, layout, busy })

  async function reconcile() {
    publish(true)
    try {
      while (needsSync || nativeOpen !== (wanted !== 'closed')) {
        const open = wanted !== 'closed'
        const version = resetVersion
        needsSync = false
        const result = await setOpen(open)
        if (version !== resetVersion) continue
        nativeOpen = open
        layout = result
        // 等待收起时先隐藏内容，避免快速切换闪回过期菜单。
        state = wanted === 'closed' ? 'closed' : nativeOpen ? wanted : 'closed'
        publish(true)
      }
      state = wanted
    } catch (cause) {
      wanted = state
      onError(cause)
    } finally { publish(false) }
  }
  function pump() {
    if (!running) {
      running = reconcile().finally(() => {
        running = null
        // 同层切换可能同步结束；处理 finally 前同一轮到达的新目标。
        if (wanted !== state || needsSync) return pump()
      })
    }
    return running
  }
  return {
    dispatch(action) {
      wanted = transitionMenu(wanted, action)
      return pump()
    },
    reset(snapshot) {
      resetVersion++
      state = wanted = 'closed'
      nativeOpen = false
      layout = snapshot
      // 通知和命令响应走不同通道；显式收起可消除通知迟到留下的展开窗口。
      needsSync = true
      publish(Boolean(running))
      return pump()
    },
  }
}
