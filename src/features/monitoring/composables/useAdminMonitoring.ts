import { computed, ref, shallowRef, watch, type Ref } from 'vue'
import { errorMessage } from '../../../common/tauri'
import { listAudit } from '../../audit/api/audit'
import type { AuditEvent } from '../../audit/model/audit'
import type { Connection } from '../../connection/model/connection'
import { readMonitoring } from '../api/monitoring'
import { endpointReadings } from '../model/dashboard'
import type { MonitoringView } from '../model/monitoring'

export function useAdminMonitoring(profiles: Readonly<Ref<Connection[] | undefined>>) {
  const views = shallowRef<Record<string, MonitoringView>>({})
  const errors = shallowRef<Record<string, string>>({})
  const activity = shallowRef<AuditEvent[]>([])
  const activityError = ref('')
  const pending = ref(false)
  const endpoints = computed(() => endpointReadings(profiles.value ?? [], views.value, errors.value))
  let sequence = 0

  watch(profiles, () => {
    views.value = {}
    errors.value = {}
    void refresh()
  }, { immediate: true })

  async function refresh(): Promise<void> {
    const cycle = ++sequence
    const selected = profiles.value
    if (!selected) return
    pending.value = true
    activityError.value = ''
    const [audit, ...responses] = await Promise.allSettled([listAudit(), ...selected.map(profile => readMonitoring(profile.id))] as const)
    if (cycle !== sequence) return
    if (audit.status === 'fulfilled') activity.value = audit.value
    else activityError.value = errorMessage(audit.reason)
    const next: Record<string, MonitoringView> = {}
    const failures: Record<string, string> = {}
    for (const [index, response] of responses.entries()) {
      const profile = selected[index]!
      if (response.status === 'rejected') failures[profile.id] = errorMessage(response.reason)
      else if (response.value.scope.connectionId === profile.id && response.value.scope.revision === (profile.revision ?? null)) {
        next[profile.id] = response.value
      }
    }
    views.value = next
    errors.value = failures
    pending.value = false
  }

  return { views, endpoints, activity, activityError, pending, refresh }
}
