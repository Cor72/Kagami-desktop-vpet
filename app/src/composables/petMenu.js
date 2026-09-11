const rootItems = [
  { id: 'expressions', label: '表情', icon: 'smile', submenu: true },
  { id: 'settings', label: '设置', icon: 'settings' },
  { id: 'always-on-top', label: '置顶', icon: 'pin' },
  { id: 'hide', label: '隐藏', icon: 'eye-off' },
  { id: 'quit', label: '退出', icon: 'power', danger: true },
]
const expressionItems = [
  { id: 'smile', label: '微笑', icon: 'smile' },
  { id: 'squint', label: '眯眼', icon: 'laugh' },
  { id: 'tears', label: '泪眼', icon: 'frown' },
  { id: 'teardrop', label: '泪滴', icon: 'droplet' },
  { id: 'back', label: '返回', icon: 'arrow-left' },
]

export function pickExpression(expressions, previous, random = Math.random) {
  if (!expressions.length) return null
  const candidates = expressions.filter(name => name !== previous)
  const pool = candidates.length ? candidates : expressions
  return pool[Math.floor(random() * pool.length)]
}

export function transitionMenu(state, action) {
  if (action === 'close') return 'closed'
  if (action === 'context') return state === 'closed' ? 'root' : 'closed'
  if (action === 'escape') return state === 'expressions' ? 'root' : 'closed'
  if (action === 'back' && state === 'expressions') return 'root'
  if (action === 'expressions' && state === 'root') return 'expressions'
  return state
}

export function getMenuItems(state) {
  return state === 'root' ? rootItems : state === 'expressions' ? expressionItems : []
}
