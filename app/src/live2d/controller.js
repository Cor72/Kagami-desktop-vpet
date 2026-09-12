import { convertFileSrc } from '@tauri-apps/api/core'
import { join } from '@tauri-apps/api/path'
import { exists, readTextFile } from '@tauri-apps/plugin-fs'
import { Config, CubismSetting, Live2DSprite } from 'easy-live2d'
import { Application } from 'pixi.js'
import { getPetCursorPosition } from '../api/pet.js'
import { findExpressionIndex, fitModel, getModelFiles } from './modelConfig.js'
import { createMouseFollow } from './mouseFollow.js'

// 使用桌面全局坐标驱动跟随，关闭库内需要按住鼠标的输入，避免两路目标相互覆盖。
Config.MouseFollow = false
Config.MotionSound = false
Config.DebugLogEnable = import.meta.env.DEV

// Cubism 使用全局运行环境；旧的异步加载结束并清理后，才开始新的加载。
let previousCreation = Promise.resolve()

export function createPetController(options) {
  const creation = previousCreation.then(() => loadPet(options))
  previousCreation = creation.catch(() => {})
  return creation
}

async function loadPet({ canvas, modelDirectory, signal }) {
  let app
  let model
  let detachProbe
  let mouseFollow
  const updateMouseFollow = () => { void mouseFollow.update() }
  let disposed = false

  function destroy() {
    if (disposed) return
    disposed = true
    app?.stop()
    mouseFollow?.destroy()
    app?.ticker.remove(updateMouseFollow)
    detachProbe?.()
    model?.destroy()
    // Canvas 属于 Vue，不让 Pixi 删除 DOM。
    if (app?.renderer) app.destroy({ removeView: false }, { children: true })
  }

  try {
    signal?.throwIfAborted()
    if (!globalThis.Live2DCubismCore) throw new Error('Cubism Core 未加载，请检查 public/live2d/')

    const modelPath = await join(modelDirectory, 'yachiyo.model3.json')
    const modelJSON = JSON.parse(await readTextFile(modelPath))
    const files = getModelFiles(modelJSON)
    const urls = new Map()
    for (const file of files) {
      const path = await join(modelDirectory, file)
      if (!await exists(path)) throw new Error(`缺少模型文件：${path}`)
      urls.set(file, convertFileSrc(path))
    }
    signal?.throwIfAborted()

    const modelSetting = new CubismSetting({ modelJSON })
    modelSetting.redirectPath(({ file }) => {
      const url = urls.get(file)
      if (!url) throw new Error(`未预检的模型文件：${file}`)
      return url
    })

    app = new Application()
    await app.init({
      canvas,
      width: canvas.clientWidth || 320,
      height: canvas.clientHeight || 420,
      preference: 'webgl',
      backgroundAlpha: 0,
      resolution: 1,
      antialias: true,
      autoStart: false,
      sharedTicker: false,
    })
    signal?.throwIfAborted()
    app.ticker.maxFPS = 30
    model = new Live2DSprite({ modelSetting, ticker: app.ticker })
    app.stage.addChild(model)
    // 首次绘制会触发库的初始化，因此必须先启动，再等待 ready。
    app.start()
    await model.ready
    signal?.throwIfAborted()

    const size = model.getModelCanvasSize()
    if (!size?.width || !size?.height) throw new Error('模型初始化后没有有效画布尺寸')
    model.anchor.set(0.5)

    function resize(width, height) {
      if (disposed || width <= 0 || height <= 0) return
      app.renderer.resize(width, height)
      const layout = fitModel(size.width, size.height, width, height)
      // 八千代的原始画布留白较多；修改这个倍率即可调整角色大小。
      const modelZoom = 4
      model.scale.set(layout.scale * modelZoom)
      model.position.set(layout.x, layout.y+140)
    }

    resize(canvas.clientWidth, canvas.clientHeight)

    // easy-live2d 0.4.4 未公开 focus API；固定版本的 Cubism 模型提供 setDragging。
    // 只设置目标，由 Cubism 平滑并在物理计算前叠加头、眼和身体参数，保留表情/呼吸。
    if (typeof model._model?.setDragging !== 'function') throw new Error('当前 Live2D 运行库不支持视线跟随')
    mouseFollow = createMouseFollow({
      readPointer: getPetCursorPosition,
      getBounds: () => canvas.getBoundingClientRect(),
      setFocus: (x, y) => model._model.setDragging(x, y),
      onError: cause => console.warn('[Live2D] 鼠标跟随暂时不可用，将自动重试：', cause),
    })
    app.ticker.add(updateMouseFollow)

    // 仅专项测量构建加载探针；正常发布版没有计数上报或采样定时器。
    if (import.meta.env.VITE_PERF_AUDIT === '1') {
      const { attachPerformanceProbe } = await import('./performanceProbe.js')
      detachProbe = attachPerformanceProbe(model, app)
    }

    return {
      async setExpression(name) {
        if (disposed) return
        const index = findExpressionIndex(modelJSON.FileReferences.Expressions ?? [], name)
        model.setExpression({ index })
      },
      resize,
      setRunning(running) {
        if (disposed) return
        mouseFollow.setRunning(running)
        if (running === app.ticker.started) return
        if (running) {
          model.resetTime()
          app.ticker.lastTime = performance.now()
          app.start()
        } else {
          app.stop()
        }
      },
      setMaxFps(fps) {
        if (fps !== 15 && fps !== 30) throw new Error('帧率只能是 15 或 30')
        if (!disposed) app.ticker.maxFPS = fps
      },
      destroy,
    }
  } catch (cause) {
    destroy()
    throw cause
  }
}
