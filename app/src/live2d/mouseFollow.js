// 输入和 DOMRect 都使用窗口客户区的逻辑像素，与 Pixi 渲染分辨率无关。
export function getFocusTarget(pointer, bounds) {
  if (bounds.width <= 0 || bounds.height <= 0) return { x: 0, y: 0 }
  const x = (pointer.x - bounds.left - bounds.width / 2) / (bounds.width / 2)
  const y = (bounds.top + bounds.height / 2 - pointer.y) / (bounds.height / 2)
  if (!Number.isFinite(x) || !Number.isFinite(y)) return { x: 0, y: 0 }
  // 单位圆限制远处/对角线的转头幅度；中心附近保留细微移动。
  const length = Math.max(1, Math.hypot(x, y))
  return { x: x / length, y: y / length }
}

// 由现有渲染 ticker 调用，不创建额外的动画循环或后台定时器。
export function createMouseFollow({ readPointer, getBounds, setFocus, onError, now = () => performance.now() }) {
  let running = false
  let disposed = false
  let pending = false
  let generation = 0
  let retryAt = 0
  let reportedError = false

  return {
    async update() {
      if (!running || disposed || pending || now() < retryAt) return
      pending = true
      const version = generation
      try {
        const pointer = await readPointer()
        if (disposed || !running || version !== generation) return
        const target = getFocusTarget(pointer, getBounds())
        setFocus(target.x, target.y)
        reportedError = false
      } catch (cause) {
        if (disposed || !running || version !== generation) return
        setFocus(0, 0)
        retryAt = now() + 1000
        if (!reportedError) {
          reportedError = true
          onError(cause)
        }
      } finally {
        pending = false
      }
    },
    setRunning(value) {
      if (disposed || running === value) return
      running = value
      generation++
      retryAt = 0
      if (!running) setFocus(0, 0)
    },
    destroy() {
      disposed = true
      running = false
      generation++
    },
  }
}
