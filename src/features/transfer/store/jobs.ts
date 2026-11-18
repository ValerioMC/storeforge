import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import { errorMessage } from '../../../common/tauri'
import { transfersApi } from '../api/transfers'
import type { Job, Transfer } from '../model/transfer'

export const useJobs = defineStore('jobs', () => {
  const jobs = ref<Job[]>([])
  const error = ref('')
  const active = computed(() => jobs.value.filter(job => job.status === 'running').length)

  async function load(): Promise<void> {
    try {
      jobs.value = await transfersApi.jobs()
      error.value = ''
    } catch (failure) {
      error.value = errorMessage(failure)
    }
  }

  async function start(transfer: Transfer): Promise<string> {
    const id = await transfersApi.start(transfer)
    await load()
    return id
  }

  async function cancel(id: string): Promise<void> {
    await transfersApi.cancel(id)
    await load()
  }

  return { jobs, active, error, load, start, cancel }
})
