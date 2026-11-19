import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import { connectionsApi } from '../api/connections'
import { demoConnection } from '../../browser/model/demo'
import { sameRealmEndpoints, type Connection, type ConnectionDraft } from '../model/connection'

export const useConnections = defineStore('connections', () => {
  const profiles = ref<Connection[]>([])
  const activeId = ref('')
  let latestRefresh = 0
  const active = computed(() => activeId.value === 'demo' ? demoConnection : profiles.value.find(connection => connection.id === activeId.value))

  const contextKey = computed(() => active.value ? JSON.stringify([active.value.id, active.value.revision]) : '')
  const realmEndpoints = computed(() => sameRealmEndpoints(active.value, profiles.value))

  async function load(): Promise<void> {
    await refreshProfiles()
    const remembered = window.localStorage.getItem('storeforge.connection')
    const preferred = profiles.value.find(profile => profile.id === remembered)
    activeId.value = preferred?.id ?? defaultProfileId()
  }

  function defaultProfileId(): string {
    const saved = profiles.value.filter(profile => !profile.settings?.openstack)
    return saved.find(profile => profile.settings?.defaultEndpoint)?.id ?? saved[0]?.id ?? ''
  }

  function select(id: string): void {
    activeId.value = id
    if (active.value && id !== 'demo' && !active.value.settings?.openstack) window.localStorage.setItem('storeforge.connection', id)
  }

  async function save(draft: ConnectionDraft): Promise<Connection> {
    const profile = await connectionsApi.save(draft)
    await refreshProfiles()
    return profile
  }

  async function refreshProfile(id: string): Promise<Connection | undefined> {
    await refreshProfiles()
    return profiles.value.find(profile => profile.id === id)
  }

  async function refreshProfiles(openStackOnly = false): Promise<void> {
    const request = openStackOnly ? latestRefresh : ++latestRefresh
    const updated = await connectionsApi.list()
    if (request !== latestRefresh) return
    if (openStackOnly) {
      const projects = updated.filter(profile => profile.settings?.openstack)
      const current = profiles.value.filter(profile => profile.settings?.openstack)
      if (projects.length === current.length && projects.every((profile, index) => profile.id === current[index]?.id)) return
      profiles.value = [...profiles.value.filter(profile => !profile.settings?.openstack), ...projects]
    } else {
      profiles.value = updated
    }
  }

  async function remove(id: string): Promise<void> {
    await connectionsApi.remove(id)
    await refreshProfiles()
    if (activeId.value === id) select(defaultProfileId())
  }

  return { profiles, activeId, active, contextKey, realmEndpoints, load, select, save, refreshProfile, refreshProfiles, remove }
})
