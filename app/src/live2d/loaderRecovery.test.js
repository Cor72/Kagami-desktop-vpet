import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'
import vm from 'node:vm'

test('一张贴图失败后，等其余贴图结束才允许释放模型', async () => {
  // 这是针对锁定版本依赖补丁的回归测试，直接执行其真实加载器。
  // 只截取不依赖 DOM 的类；升级 easy-live2d 时需同步检查这一测试入口。
  const source = await readFile(new URL(import.meta.resolve('easy-live2d')), 'utf8')
  const start = source.indexOf('var ModelLoader = class')
  const end = source.indexOf('var TextureLoader = class', start)
  assert.ok(start >= 0 && end > start, '依赖加载器结构已改变，需要复核补丁')
  const ModelLoader = vm.runInNewContext(`${source.slice(start, end)}; ModelLoader`)

  const callbacks = []
  const boundTextures = []
  const model = { bindTexture: (index, texture) => boundTextures.push([index, texture]) }
  const textureLoader = {
    createTextureFromPngFile: (url, premultiply, resolve, reject) => callbacks.push({ resolve, reject }),
  }
  const context = {
    setting: { getTextureCount: () => 2, getTextureFileName: index => `${index}.png` },
    redir: { Textures: [] }, homeDir: '/model/',
  }
  let outcome = 'pending'
  const loading = new ModelLoader().loadTextures(model, context, textureLoader)
    .then(() => { outcome = 'ready' }, () => { outcome = 'failed' })

  callbacks[0].reject(new Error('损坏的 PNG'))
  await new Promise(resolve => setImmediate(resolve))
  const beforeSecondTexture = outcome
  callbacks[1].resolve({ id: 'second-texture' })
  await loading

  assert.equal(beforeSecondTexture, 'pending', '不能提前销毁第二张贴图仍在使用的 WebGL 上下文')
  assert.equal(outcome, 'failed')
  assert.deepEqual(boundTextures, [[1, 'second-texture']])
})
