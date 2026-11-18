import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import { connectionsApi } from '../api/connections'
import { demoConnection } from '../../browser/model/demo'
import type { Connection, ConnectionDraft } from '../model/connection'

export const useConnections = defineStore('connections', () => {
  const profiles = ref<Connection[]>([])
  const activeId = ref('')
  const active = computed(() => activeId.value === 'demo' ? demoConnection : profiles.value.find(connection => connection.id === activeId.value))

  async function load(): Promise<void> {
    profiles.value = await connectionsApi.list()
    const remembered = localStorage.getItem('storeforge.connection')
    const preferred = profiles.value.find(profile => profile.id === remembered)
    activeId.value = preferred?.id ?? profiles.value[0]?.id ?? ''
  }

  function select(id: string): void {
    activeId.value = id
    if (id !== 'demo') localStorage.setItem('storeforge.connection', id)
  }

  async function save(draft: ConnectionDraft): Promise<Connection> {
    const profile = await connectionsApi.save(draft)
    profiles.value = await connectionsApi.list()
    select(profile.id)
    return profile
  }

  async function remove(id: string): Promise<void> {
    await connectionsApi.remove(id)
    profiles.value = await connectionsApi.list()
    if (activeId.value === id) select(profiles.value[0]?.id ?? '')
  }

  return { profiles, activeId, active, load, select, save, remove }
})
