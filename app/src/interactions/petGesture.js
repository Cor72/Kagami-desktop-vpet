export function createPetGesture({ threshold = 6 } = {}) {
  let start = null
  let dragging = false
  const distance = event => Math.hypot(event.clientX - start.x, event.clientY - start.y)
  function cancel() { start = null; dragging = false }
  return {
    pointerDown(event) {
      cancel()
      if (event.button === 0) start = { x: event.clientX, y: event.clientY, id: event.pointerId }
      return 'none'
    },
    pointerMove(event) {
      if (!start || start.id !== event.pointerId) return 'none'
      if (!(event.buttons & 1)) { cancel(); return 'none' }
      if (!dragging && distance(event) >= threshold) {
        dragging = true
        return 'drag'
      }
      return 'none'
    },
    pointerUp(event) {
      if (!start || start.id !== event.pointerId) return 'none'
      const clicked = event.button === 0 && !dragging && distance(event) < threshold
      cancel()
      return clicked ? 'click' : 'none'
    },
    cancel,
  }
}
