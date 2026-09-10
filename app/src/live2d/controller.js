import { convertFileSrc } from '@tauri-apps/api/core'
import { join } from '@tauri-apps/api/path'
import { exists, readTextFile } from '@tauri-apps/plugin-fs'
import { Config, CubismSetting, Live2DSprite } from 'easy-live2d'
import { Application } from 'pixi.js'
import { findExpressionIndex, fitModel, getModelFiles } from './modelConfig.js'

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
  let disposed = false

  function destroy() {
    if (disposed) return
    disposed = true
    app?.stop()
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
      const modelZoom = 2.6
      model.scale.set(layout.scale * modelZoom)
      model.position.set(layout.x, layout.y)
    }

    resize(canvas.clientWidth, canvas.clientHeight)

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
