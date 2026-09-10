import test from 'node:test'
import assert from 'node:assert/strict'
import { getModelFiles, findExpressionIndex, fitModel } from './modelConfig.js'

test('预检包含模型引用的纹理、物理和表情，保留子目录', () => {
  const files = getModelFiles({ FileReferences: {
    Moc: 'pet.moc3', Textures: ['textures/0.png'], Physics: 'pet.physics3.json',
    Expressions: [{ Name: 'smile', File: 'smile.exp3.json' }],
  } })
  assert.deepEqual(files, ['pet.moc3', 'textures/0.png', 'pet.physics3.json', 'smile.exp3.json'])
})

test('模型文件路径不能离开模型目录', () => {
  for (const path of ['../secret.png', 'C:/secret.png', 'https://example.com/a.png']) {
    assert.throws(() => getModelFiles({ FileReferences: { Moc: 'pet.moc3', Textures: [path] } }))
  }
})

test('表情按名称查找，配置顺序变化后仍能选中正确表情', () => {
  const expressions = [{ Name: 'tears' }, { Name: 'smile' }]
  assert.equal(findExpressionIndex(expressions, 'smile'), 1)
  assert.throws(() => findExpressionIndex(expressions, 'unknown'))
})

test('等比缩放使整个模型落在画布内', () => {
  assert.deepEqual(fitModel(1000, 2000, 300, 400), { scale: 0.2, x: 150, y: 200 })
  assert.deepEqual(fitModel(2000, 1000, 300, 400), { scale: 0.15, x: 150, y: 200 })
})
