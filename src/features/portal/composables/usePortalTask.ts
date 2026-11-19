import { onBeforeUnmount, ref, shallowRef } from 'vue'
import { errorMessage } from '../../../common/tauri'

export function usePortalTask<T>() {
  const data = shallowRef<T>()
  const pending = ref(false)
  const error = ref('')
  let generation = 0

  function clear(): void {
    generation++
    data.value = undefined
    pending.value = false
    error.value = ''
  }

  async function run(load: () => Promise<T>): Promise<T | undefined> {
    const request = ++generation
    pending.value = true
    error.value = ''
    try {
      const result = await load()
      if (request !== generation) return
      data.value = result
      return result
    } catch (failure) {
      if (request === generation) error.value = errorMessage(failure)
    } finally {
      if (request === generation) pending.value = false
    }
  }

  onBeforeUnmount(clear)
  return { data, pending, error, clear, run }
}
