export function getModelFiles(modelJSON) {
  const refs = modelJSON.FileReferences
  if (!refs?.Moc || !refs.Textures?.length) throw new Error('模型配置缺少 Moc 或 Textures')

  const files = [
    refs.Moc, ...refs.Textures, refs.Physics, refs.DisplayInfo,
    ...(refs.Expressions ?? []).map(expression => expression.File),
  ].filter(Boolean)

  for (const file of files) {
    if (typeof file !== 'string' || file.startsWith('/') || /[:\\]/.test(file) || file.split('/').includes('..')) {
      throw new Error(`模型引用必须是目录内的相对路径：${file}`)
    }
  }
  return [...new Set(files)]
}

export function findExpressionIndex(expressions, name) {
  const index = expressions.findIndex(expression => expression.Name === name)
  if (index < 0) throw new Error(`模型中没有表情：${name}`)
  return index
}

export function fitModel(modelWidth, modelHeight, width, height) {
  return { scale: Math.min(width / modelWidth, height / modelHeight), x: width / 2, y: height / 2 }
}
