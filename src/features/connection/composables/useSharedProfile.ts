import { computed, ref } from 'vue'
import { errorMessage } from '../../../common/tauri'
import { sharedServiceApi } from '../api/shared-service'
import type { SharedServiceStatus } from '../model/shared-service'

export function useSharedProfile(eligible: (status: SharedServiceStatus) => boolean) {
  const profiles = ref<SharedServiceStatus[]>([])
  const serviceId = ref('')
  const profile = computed(() => profiles.value.find(entry => entry.profile.id === serviceId.value))
  const options = computed(() => profiles.value.map(({ profile }) => ({ value: profile.id, label: profile.name })))
  const pending = ref(false)
  const error = ref('')

  async function loadProfiles(): Promise<void> {
    pending.value = true
    error.value = ''
    try {
      profiles.value = await sharedServiceApi.list()
      if (!profile.value) serviceId.value = (profiles.value.find(eligible) ?? profiles.value[0])?.profile.id ?? ''
    } catch (failure) {
      error.value = errorMessage(failure)
    } finally {
      pending.value = false
    }
  }

  return { serviceId, profile, options, pending, error, loadProfiles }
}
