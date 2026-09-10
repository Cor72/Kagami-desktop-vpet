export function selectRenderPolicy(nativeVisible, pageVisible, maxFps, minimized = false) {
  return { running: nativeVisible && pageVisible && !minimized, maxFps }
}
