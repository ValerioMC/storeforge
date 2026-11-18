<script setup lang="ts">
import { computed, ref } from 'vue'
import { Activity, Box, Cable, ChevronDown, Database, FolderCog, KeyRound, LayoutDashboard, Plus, Search, Settings2, ShieldCheck, Star } from 'lucide-vue-next'
import icon from '../../assets/storeforge.png'
import { useBrowser } from '../../features/browser/store/browser'
import { useConnections } from '../../features/connection/store/connections'
import { workspaceAvailable, type WorkspaceDestination, type WorkspaceRoute } from '../../features/connection/model/workspace'

const props = defineProps<{ route: WorkspaceRoute; demo: boolean; hasDraft: boolean; dragRegion?: string }>()
const emit = defineEmits<{ add: []; go: [destination: WorkspaceDestination]; bucket: [name: string]; createBucket: [] }>()
const connections = useConnections()
const browser = useBrowser()
const search = ref('')
const favoritesOnly = ref(false)
const favorites = ref<string[]>(loadFavorites())

const storageLinks = [
  { label: 'Browser', icon: Box, destination: { workspace: 'browser' } },
  { label: 'Overview', icon: LayoutDashboard, destination: { workspace: 'manager', page: 'overview' } },
  { label: 'Bucket inventory', icon: Database, destination: { workspace: 'manager', page: 'buckets' } },
  { label: 'Storage tools', icon: FolderCog, destination: { workspace: 'manager', page: 'tools' } },
] as const
const adminLinks = [
  { label: 'Access & topics', icon: KeyRound, destination: { workspace: 'manager', page: 'access' } },
  { label: 'Ceph Admin', icon: ShieldCheck, destination: { workspace: 'cephAdmin' } },
  { label: 'Connections', icon: Cable, destination: { workspace: 'admin', page: 'connections' } },
  { label: 'Activity', icon: Activity, destination: { workspace: 'admin', page: 'activity' } },
  { label: 'Preferences', icon: Settings2, destination: { workspace: 'admin', page: 'preferences' } },
] as const
const groups = computed(() => [
  { id: 'storage' as const, name: 'Storage', links: storageLinks.filter(({ destination }) => workspaceAvailable(connections.active, destination.workspace)) },
  { id: 'administration' as const, name: 'Administration', links: adminLinks.filter(({ destination }) => workspaceAvailable(connections.active, destination.workspace)) },
].filter(group => group.links.length))
const expanded = ref({ storage: groupOpen('storage'), administration: groupOpen('administration'), buckets: groupOpen('buckets') })
const buckets = computed(() => browser.buckets.filter(bucket =>
  bucket.name.toLowerCase().includes(search.value.toLowerCase()) && (!favoritesOnly.value || isFavorite(bucket.name))))

function groupOpen(id: string): boolean {
  return window.localStorage.getItem(`storeforge.sidebar.${id}`) !== 'closed'
}

function setGroup(id: keyof typeof expanded.value, event: Event): void {
  const open = (event.currentTarget as HTMLDetailsElement).open
  expanded.value[id] = open
  window.localStorage.setItem(`storeforge.sidebar.${id}`, open ? 'open' : 'closed')
}

function activeLink(destination: WorkspaceDestination): boolean {
  if (props.route.workspace !== destination.workspace) return false
  if ('page' in destination) return 'page' in props.route && props.route.page === destination.page
  return true
}

function loadFavorites(): string[] {
  try {
    const stored: unknown = JSON.parse(window.localStorage.getItem('storeforge.favorites') ?? '[]')
    return Array.isArray(stored) ? stored.filter((item): item is string => typeof item === 'string') : []
  } catch {
    return []
  }
}

function favoriteId(bucket: string): string {
  return `${connections.activeId}/${bucket}`
}

function isFavorite(bucket: string): boolean {
  return favorites.value.includes(favoriteId(bucket))
}

function toggleFavorite(bucket: string): void {
  const id = favoriteId(bucket)
  favorites.value = isFavorite(bucket) ? favorites.value.filter(value => value !== id) : [...favorites.value, id]
  window.localStorage.setItem('storeforge.favorites', JSON.stringify(favorites.value))
}
</script>

<template>
  <aside class="sidebar" aria-label="Storage navigation">
    <div class="brand" :data-tauri-drag-region="dragRegion"><img :src="icon" alt="" width="36" height="36" draggable="false" /><strong>StoreForge</strong></div>

    <div class="sidebar-scroll">
      <nav aria-label="Workspaces">
        <details v-for="group in groups" :key="group.id" class="sidebar-group" :open="expanded[group.id]"
          @toggle="setGroup(group.id, $event)">
          <summary>{{ group.name }}<ChevronDown :size="14" /></summary>
          <button v-for="{ label, icon: linkIcon, destination } in group.links" :key="label" class="rail-link"
            :class="{ active: activeLink(destination) }" :aria-current="activeLink(destination) ? 'page' : undefined" @click="emit('go', destination)">
            <component :is="linkIcon" :size="16" /><span>{{ label }}</span>
          </button>
        </details>
      </nav>

      <details v-if="route.workspace === 'browser'" class="sidebar-group sidebar-buckets" :open="expanded.buckets" @toggle="setGroup('buckets', $event)">
        <summary>Buckets <span v-if="browser.buckets.length" class="rail-count">{{ browser.buckets.length }}</span><ChevronDown :size="14" /></summary>
        <div class="bucket-controls">
          <label class="sidebar-search"><Search :size="15" /><input v-model="search" type="search" placeholder="Find a bucket" aria-label="Find a bucket" /></label>
          <button class="icon-button subtle" :class="{ 'side-button-active': favoritesOnly }" :aria-pressed="favoritesOnly" aria-label="Show favorites" title="Show favorites" @click="favoritesOnly = !favoritesOnly"><Star :size="16" /></button>
          <button class="icon-button subtle" aria-label="Create bucket" title="Create bucket" :disabled="demo" @click="emit('createBucket')"><Plus :size="16" /></button>
        </div>
        <div class="bucket-list">
          <p v-if="browser.bucketLoading" class="side-message" role="status">Reading buckets…</p>
          <p v-else-if="browser.bucketError" class="side-message" role="status">Bucket listing unavailable.</p>
          <p v-else-if="!buckets.length" class="side-message">{{ browser.buckets.length ? 'No matching buckets.' : 'No buckets available.' }}</p>
          <div v-for="bucket in buckets" :key="bucket.name" class="bucket-link" :class="{ active: browser.bucket === bucket.name }">
            <button class="bucket-target" :title="bucket.name" :aria-current="browser.bucket === bucket.name ? 'location' : undefined" @click="emit('bucket', bucket.name)">
              <Database :size="16" /><span>{{ bucket.name }}</span>
            </button>
            <button class="favorite-button subtle" :class="{ starred: isFavorite(bucket.name) }" :aria-label="`${isFavorite(bucket.name) ? 'Remove favorite' : 'Favorite'} ${bucket.name}`" @click="toggleFavorite(bucket.name)">
              <Star :size="13" />
            </button>
          </div>
        </div>
      </details>
    </div>
    <div class="sidebar-footer">
      <button class="side-action" @click="emit('add')"><Cable :size="17" /> {{ hasDraft ? 'Resume setup' : 'Add connection' }}<Plus class="side-end" :size="15" /></button>
    </div>
  </aside>
</template>
