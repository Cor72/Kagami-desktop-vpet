import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'

const directory = resolve(process.argv[2] ?? '../docs/performance/2026-09-10')
const readLines = name => readFileSync(resolve(directory, name), 'utf8').replace(/^\uFEFF/, '').trim().split(/\r?\n/).filter(Boolean).map(JSON.parse)
const frames = readLines('renderer.jsonl')
const processes = readLines('processes.jsonl')
// 始终从逐进程原始值求和，也兼容早期采样脚本中为空的总计字段。
for (const row of processes) {
  row.workingSetBytes = row.processes.reduce((sum, item) => sum + item.workingSetBytes, 0)
  row.privateBytes = row.processes.reduce((sum, item) => sum + item.privateBytes, 0)
}
const mean = values => values.length ? values.reduce((a, b) => a + b, 0) / values.length : null
const median = values => {
  const sorted = [...values].sort((a, b) => a - b)
  return sorted.length ? (sorted[Math.floor((sorted.length - 1) / 2)] + sorted[Math.floor(sorted.length / 2)]) / 2 : null
}
const mib = bytes => bytes / 1024 ** 2
const results = []
for (const phase of [...new Set(frames.map(row => row.phase))]) {
  if (phase.startsWith('cycle-')) continue
  const records = frames.filter(row => row.phase === phase)
  const start = records[0].timestampMs
  const end = records.at(-1).timestampMs
  // 越过状态切换与跨采样区间，不把上一阶段的最后一条指标归到新状态。
  const stable = records.filter(row => row.timestampMs >= start + 10000)
  const reports = [...new Map(stable.map(row => [row.renderer.reportedAt, row.renderer])).values()]
    .filter(row => row.reportedAt >= start + 5000 && row.reportedAt <= end)
  const first = reports[0]
  const last = reports.at(-1)
  const duration = first && last ? (last.reportedAt - first.reportedAt) / 1000 : 0
  if (phase === 'idle-30-minutes') {
    const interruption = records.find(row => !row.nativeVisible || row.minimized || !row.settings.visible || row.settings.maxFps !== 30)
    const stale = stable.some(row => row.timestampMs - row.renderer.reportedAt > 10000)
    const paused = reports.some(row => !row.running || !row.pageVisible || row.maxFps !== 30)
      || (first && last && first.pauses !== last.pauses)
    if (interruption || stale || paused || end - start < 1800000) {
      results.push({
        phase, phaseSeconds: (end - start) / 1000, continuousVisibleIdlePassed: false,
        reason: interruption || paused ? 'visible idle interrupted' : stale ? 'stale renderer reports' : 'less than 30 minutes',
        firstNativeInterruption: interruption ? {
          elapsedSeconds: interruption.elapsedSeconds, timestampMs: interruption.timestampMs,
          settings: interruption.settings, nativeVisible: interruption.nativeVisible, minimized: interruption.minimized,
        } : null,
      })
      continue
    }
  }
  const os = processes.filter(row => row.timestampMs >= start + 10000 && row.timestampMs <= end)
  const gpu = key => os.filter(row => !row.gpuCounterError && row.gpuMemory.length).map(row => mib(row.gpuMemory.reduce((sum, item) => sum + Number(item[key]), 0)))
  const minuteGroups = new Map()
  for (const row of os) {
    const minute = Math.floor((row.timestampMs - start) / 60000)
    if (!minuteGroups.has(minute)) minuteGroups.set(minute, [])
    minuteGroups.get(minute).push(row)
  }
  results.push({
    phase, phaseSeconds: (end - start) / 1000, measuredRenderSeconds: duration,
    updatesPerSecond: duration ? (last.updates - first.updates) / duration : null,
    drawsPerSecond: duration ? (last.draws - first.draws) / duration : null,
    updateDelta: first && last ? last.updates - first.updates : null,
    drawDelta: first && last ? last.draws - first.draws : null,
    instanceCount: new Set(records.map(row => row.renderer.instanceId)).size,
    runningStates: [...new Set(reports.map(row => row.running))],
    cpuPercentMean: mean(os.map(row => row.cpuPercent)),
    workingSetMiBMedian: median(os.map(row => mib(row.workingSetBytes))),
    privateMiBMedian: median(os.map(row => mib(row.privateBytes))),
    gpuDedicatedMiBMedian: median(gpu('DedicatedUsage')),
    gpuSharedMiBMedian: median(gpu('SharedUsage')),
    osSamples: os.length,
    startPrivateMiB: median(os.slice(0, 12).map(row => mib(row.privateBytes))),
    endPrivateMiB: median(os.slice(-12).map(row => mib(row.privateBytes))),
    minuteMedians: phase === 'idle-30-minutes' ? [...minuteGroups].map(([minute, rows]) => ({
      minute, samples: rows.length,
      cpuPercent: mean(rows.map(row => row.cpuPercent)),
      privateMiB: median(rows.map(row => mib(row.privateBytes))),
      workingSetMiB: median(rows.map(row => mib(row.workingSetBytes))),
    })) : undefined,
  })
}
const summary = {
  results,
  completedAllPhases: existsSync(resolve(directory, 'complete.json')),
  deferred: existsSync(resolve(directory, 'deferred.json')),
  totalInstanceCount: new Set(frames.map(row => row.renderer.instanceId)).size,
  visibilityCycles: new Set(frames.filter(row => /^cycle-\d+-visible$/.test(row.phase)).map(row => row.phase)).size,
  finalRenderer: frames.at(-1).renderer,
  gpuCounterErrors: [...new Set(processes.map(row => row.gpuCounterError).filter(Boolean))],
  distinctProcessNames: [...new Set(processes.flatMap(row => row.processes.map(item => item.name)))],
}
writeFileSync(resolve(directory, 'summary.json'), JSON.stringify(summary, null, 2) + '\n')
console.log(JSON.stringify(summary, null, 2))
