import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import { connectionsApi } from '../api/connections'
import { demoConnection } from '../../browser/model/demo'
import { sameRealmEndpoints, type Connection, type ConnectionDraft } from '../model/connection'

export const useConnections = defineStore('connections', () => {
  const profiles = ref<Connection[]>([])
  const activeId = ref('')
  const active = computed(() => activeId.value === 'demo' ? demoConnection : profiles.value.find(connection => connection.id === activeId.value))

  const contextKey = computed(() => active.value ? JSON.stringify([active.value.id, active.value.revision]) : '')
  const realmEndpoints = computed(() => sameRealmEndpoints(active.value, profiles.value))

  async function load(): Promise<void> {
    profiles.value = await connectionsApi.list()
    const remembered = window.localStorage.getItem('storeforge.connection')
    const preferred = profiles.value.find(profile => profile.id === remembered)
    activeId.value = preferred?.id ?? defaultProfileId()
  }

  function defaultProfileId(): string {
    return profiles.value.find(profile => profile.settings?.defaultEndpoint)?.id ?? profiles.value[0]?.id ?? ''
  }

  function select(id: string): void {
    activeId.value = id
    if (id !== 'demo') window.localStorage.setItem('storeforge.connection', id)
  }

  async function save(draft: ConnectionDraft): Promise<Connection> {
    const profile = await connectionsApi.save(draft)
    profiles.value = await connectionsApi.list()
    return profile
  }

  async function refreshProfile(id: string): Promise<Connection | undefined> {
    profiles.value = await connectionsApi.list()
    return profiles.value.find(profile => profile.id === id)
  }

  async function remove(id: string): Promise<void> {
    await connectionsApi.remove(id)
    profiles.value = await connectionsApi.list()
    if (activeId.value === id) select(defaultProfileId())
  }

  return { profiles, activeId, active, contextKey, realmEndpoints, load, select, save, refreshProfile, remove }
})
