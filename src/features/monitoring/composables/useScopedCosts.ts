import { onActivated, ref, watch, type Ref } from 'vue'
import { errorMessage } from '../../../common/tauri'
import { readScopedCosts } from '../api/storage-costs'
import type { ScopedCostSummary } from '../model/storage-costs'

export function useScopedCosts(connection: Ref<string | undefined>, month: Ref<string>) {
  const summary = ref<ScopedCostSummary | null>(null)
  const pending = ref(false)
  const error = ref('')
  let request = 0
  let loaded = false

  async function refresh(): Promise<void> {
    const id = connection.value
    const generation = ++request
    summary.value = null
    error.value = ''
    pending.value = !!id
    if (!id) return
    try {
      const result = await readScopedCosts(id, month.value)
      if (generation === request) summary.value = result
    } catch (failure) {
      if (generation === request) error.value = errorMessage(failure)
    } finally {
      if (generation === request) pending.value = false
    }
  }

  watch([connection, month], () => { loaded = true; void refresh() }, { immediate: true })
  onActivated(() => {
    if (loaded && !pending.value && connection.value) void refresh()
  })
  return { summary, pending, error }
}
