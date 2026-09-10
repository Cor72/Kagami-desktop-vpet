import { emit } from '@tauri-apps/api/event'

// 测量专用：包住锁定版本中实际更新/绘制的方法，不用 ticker 次数冒充绘制次数。
export function attachPerformanceProbe(sprite, app) {
  const model = sprite._model
  const update = model.update
  const draw = model.draw
  const resetTime = sprite.resetTime
  const start = app.start
  const stop = app.stop
  const metrics = {
    instanceId: crypto.randomUUID(), updates: 0, draws: 0, resumes: 0,
    maxDeltaSeconds: 0, maxResumeDeltaSeconds: 0, errors: 0,
    pauses: 0, maxPausedUpdateDelta: 0, maxPausedDrawDelta: 0,
  }
  let nextIsResume = false
  let pausedAt
  app.stop = function () {
    stop.call(this)
    if (!pausedAt) {
      metrics.pauses++
      pausedAt = { updates: metrics.updates, draws: metrics.draws }
    }
  }
  app.start = function () {
    if (pausedAt) {
      metrics.maxPausedUpdateDelta = Math.max(metrics.maxPausedUpdateDelta, metrics.updates - pausedAt.updates)
      metrics.maxPausedDrawDelta = Math.max(metrics.maxPausedDrawDelta, metrics.draws - pausedAt.draws)
      pausedAt = undefined
    }
    start.call(this)
  }
  model.update = function (delta) {
    const result = update.call(this, delta)
    metrics.updates++
    metrics.maxDeltaSeconds = Math.max(metrics.maxDeltaSeconds, delta)
    if (nextIsResume) {
      metrics.maxResumeDeltaSeconds = Math.max(metrics.maxResumeDeltaSeconds, delta)
      nextIsResume = false
    }
    return result
  }
  model.draw = function (...args) {
    const result = draw.apply(this, args)
    metrics.draws++
    return result
  }
  sprite.resetTime = function () {
    metrics.resumes++
    nextIsResume = true
    return resetTime.call(this)
  }
  const onError = () => { metrics.errors++ }
  window.addEventListener('error', onError)
  window.addEventListener('unhandledrejection', onError)
  const report = () => emit('pet-render-metrics', {
    ...metrics, running: app.ticker.started, maxFps: app.ticker.maxFPS,
    pageVisible: document.visibilityState === 'visible',
    canvasWidth: app.renderer.width, canvasHeight: app.renderer.height,
    reportedAt: Date.now(),
  }).catch(console.error)
  void report()
  const timer = setInterval(report, 5000)
  return () => {
    clearInterval(timer)
    window.removeEventListener('error', onError)
    window.removeEventListener('unhandledrejection', onError)
    model.update = update
    model.draw = draw
    sprite.resetTime = resetTime
    app.start = start
    app.stop = stop
  }
}
