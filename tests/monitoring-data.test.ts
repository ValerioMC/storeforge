// @vitest-environment jsdom
import { effectScope, nextTick, shallowRef } from 'vue'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { demoConnection } from '../src/features/browser/model/demo'
import { useMonitoring } from '../src/features/monitoring/composables/useMonitoring'
import type { MonitoringView } from '../src/features/monitoring/model/monitoring'

const { desktop } = vi.hoisted(() => ({ desktop: vi.fn() }))
vi.mock('../src/common/tauri', () => ({ desktop, errorMessage: String }))

const profile = { ...demoConnection, id: 'saved', revision: 'initial' }

function view(connectionId = 'saved', revision = 'initial'): MonitoringView {
  return {
    scope: { connectionId, revision, endpointId: 'endpoint', tenant: 'research', account: '', region: 'us-east-1' },
    retrievedAt: '2026-01-01T13:00:00Z',
    stale: false,
    usage: null,
    hourly: [], daily: [], health: [], incidents: [],
  }
}

describe('monitoring collection actions', () => {
  beforeEach(() => desktop.mockReset())

  it('loads and refreshes cached readings separately from usage collection and endpoint checks', async () => {
    const cached = { ...view(), stale: true }
    desktop.mockResolvedValue(cached)
    const scope = effectScope()
    const monitoring = scope.run(() => useMonitoring(shallowRef(profile)))!
    await nextTick()
    expect(monitoring.view.value?.stale).toBe(true)

    await monitoring.run('cached')
    await monitoring.run('collect')
    await monitoring.run('check', 'http')

    expect(desktop.mock.calls).toEqual([
      ['monitoring_cached', { connection: 'saved' }],
      ['monitoring_cached', { connection: 'saved' }],
      ['monitoring_collect', { connection: 'saved' }],
      ['monitoring_check', { connection: 'saved', mode: 'http' }],
    ])
    scope.stop()
  })

  it('ignores a response from a superseded connection scope', async () => {
    let resolve: (value: MonitoringView) => void = () => undefined
    desktop.mockImplementationOnce(() => new Promise<MonitoringView>(done => { resolve = done }))
      .mockResolvedValueOnce(view('saved', 'changed'))
    const selected = shallowRef(profile)
    const scope = effectScope()
    const monitoring = scope.run(() => useMonitoring(selected))!

    selected.value = { ...profile, revision: 'changed' }
    await nextTick()
    resolve(view())
    await nextTick()

    expect(monitoring.view.value?.scope.revision).toBe('changed')
    scope.stop()
  })
})
