import { computed, ref, shallowRef, watch, type Ref } from 'vue'
import { errorMessage } from '../../../common/tauri'
import type { Connection } from '../../connection/model/connection'
import { checkEndpoint, collectMonitoring, readMonitoring } from '../api/monitoring'
import type { CheckMode, MonitoringView } from '../model/monitoring'

type Action = 'cached' | 'collect' | 'check'

export function useMonitoring(profile: Readonly<Ref<Connection | undefined>>) {
  const view = shallowRef<MonitoringView>()
  const pending = ref<Action>()
  const error = ref('')
  const scope = computed(() => {
    const connection = profile.value
    return connection ? `${connection.id}:${connection.revision ?? ''}` : ''
  })
  let request = 0

  watch(scope, () => {
    request += 1
    view.value = undefined
    pending.value = undefined
    error.value = ''
    void run('cached')
  }, { immediate: true })

  async function run(action: Action, mode: CheckMode = 'signed'): Promise<void> {
    const connection = profile.value
    if (!connection || connection.id === 'demo' || pending.value) return
    const next = ++request
    const revision = connection.revision ?? null
    pending.value = action
    error.value = ''
    try {
      const received = await read(action, connection.id, mode)
      if (request === next && received.scope.connectionId === connection.id && received.scope.revision === revision) {
        view.value = received
      }
    } catch (failure) {
      if (request === next) error.value = errorMessage(failure)
    } finally {
      if (request === next) pending.value = undefined
    }
  }

  return { view, pending, error, run }
}

function read(action: Action, connection: string, mode: CheckMode): Promise<MonitoringView> {
  if (action === 'collect') return collectMonitoring(connection)
  if (action === 'check') return checkEndpoint(connection, mode)
  return readMonitoring(connection)
}
