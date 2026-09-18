// 文字气泡的纯逻辑。气泡是桌宠窗口里的一块普通 DOM，**不能主动改变窗口尺寸**，
// 所以文案必须先裁到放得下的长度再交给视图。规格来自实施计划 §3.2：
// 约 40 字、超出折行、最多 3 行。
//
// 逻辑写成纯函数是为了能用 `node --test` 直接覆盖——渲染部分在 PetSpeechBubble.vue 里。

/** 气泡文案的字数上限（含省略号）。 */
export const SPEECH_MAX_CHARS = 40

/**
 * 把任意文本整理成气泡能显示的样子：
 * 换行与连续空白压成一个空格，超长截断并补省略号。
 *
 * 用 `[...text]` 而不是 `text.slice`：中文与 emoji 都可能是两个 UTF-16 码位，
 * 按码点切才不会把一个字切成乱码。
 */
export function clampSpeechText(text, max = SPEECH_MAX_CHARS) {
  const flat = String(text ?? '')
    .replace(/\s+/g, ' ')
    .trim()
  if (max < 1) return ''
  const chars = [...flat]
  if (chars.length <= max) return flat
  return `${chars.slice(0, max - 1).join('')}…`
}

/**
 * 气泡与气泡菜单互斥：菜单开着的时候不显示气泡。
 *
 * 状态机在 `petMenu.js` 手里（`closed` / `root` / `expressions`），这里只是问一句
 * 「现在能不能显示」。不要在 PetSpeechBubble.vue 内部再存一份菜单状态。
 */
export function speechAllowed(menuState) {
  return menuState === 'closed'
}
