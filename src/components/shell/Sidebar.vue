<script setup lang="ts">
import { computed, ref } from 'vue'
import { Cable, Database, Plus, Search, Server, Settings2, Star } from 'lucide-vue-next'
import icon from '../../assets/storeforge.png'
import AppSelect from '../ui/AppSelect.vue'
import { useBrowser } from '../../features/browser/store/browser'
import { useConnections } from '../../features/connection/store/connections'
import { providerNames } from '../../features/connection/model/connection'

const props = defineProps<{ demo: boolean; browse: boolean; dragRegion?: string }>()
const emit = defineEmits<{ add: []; connections: []; bucket: [name: string]; createBucket: []; demo: [] }>()
const connections = useConnections()
const browser = useBrowser()
const search = ref('')
const favoritesOnly = ref(false)
const favorites = ref<string[]>(loadFavorites())

const connectionOptions = computed(() => [
  ...connections.profiles.map(profile => ({ value: profile.id, label: profile.name })),
  { value: 'demo', label: 'Asteria Research' },
])
const buckets = computed(() => browser.buckets.filter(bucket =>
  bucket.name.toLowerCase().includes(search.value.toLowerCase()) && (!favoritesOnly.value || isFavorite(bucket.name))))
const endpoint = computed(() => {
  const profile = connections.active
  return profile ? `${providerNames[profile.provider]} · ${profile.region}` : 'S3-compatible storage'
})

function loadFavorites(): string[] {
  try {
    const stored: unknown = JSON.parse(localStorage.getItem('storeforge.favorites') ?? '[]')
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
  localStorage.setItem('storeforge.favorites', JSON.stringify(favorites.value))
}

function selectConnection(value: string): void {
  if (value === 'demo') emit('demo')
  else connections.select(value)
}
</script>

<template>
  <aside class="sidebar" aria-label="Storage navigation">
    <div class="brand" :data-tauri-drag-region="dragRegion"><img :src="icon" alt="" width="36" height="36" draggable="false" /><strong>StoreForge</strong></div>

    <div class="connection-block">
      <label class="side-label" for="active-connection">Connection</label>
      <AppSelect id="active-connection" tone="rail" :model-value="connections.activeId" :options="connectionOptions"
        placeholder="Select a connection" @update:model-value="selectConnection">
        <template #leading><Server :size="17" /></template>
      </AppSelect>
      <div class="connection-caption"><span class="status-dot" :class="{ online: connections.active && !browser.bucketError && !browser.bucketLoading }" />{{ endpoint }}</div>
    </div>

    <div class="sidebar-section">
      <div class="sidebar-section-title">
        <h2>Buckets <span v-if="browser.buckets.length">{{ browser.buckets.length }}</span></h2>
        <button class="icon-button subtle" :class="{ 'side-button-active': favoritesOnly }" :aria-pressed="favoritesOnly" aria-label="Show favorites" title="Show favorites" @click="favoritesOnly = !favoritesOnly"><Star :size="16" /></button>
        <button class="icon-button subtle" aria-label="Create bucket" title="Create bucket" :disabled="!connections.active || props.demo" @click="emit('createBucket')"><Plus :size="17" /></button>
      </div>
      <label v-if="connections.active" class="sidebar-search"><Search :size="15" /><input v-model="search" type="search" placeholder="Find a bucket" aria-label="Find a bucket" /></label>
      <div class="bucket-list">
        <p v-if="browser.bucketLoading" class="side-message" role="status">Loading buckets…</p>
        <p v-else-if="browser.bucketError" class="side-message" role="status">Bucket listing unavailable.</p>
        <p v-else-if="!connections.active" class="side-message">Add a connection to see your buckets.</p>
        <p v-else-if="!buckets.length" class="side-message">{{ browser.buckets.length ? 'No matching buckets.' : 'No buckets available.' }}</p>
        <div v-for="bucket in buckets" :key="bucket.name" class="bucket-link" :class="{ active: browse && browser.bucket === bucket.name }">
          <button class="bucket-target" :title="bucket.name" :aria-current="browse && browser.bucket === bucket.name ? 'location' : undefined" @click="emit('bucket', bucket.name)">
            <Database :size="16" /><span>{{ bucket.name }}</span>
          </button>
          <button class="favorite-button subtle" :class="{ starred: isFavorite(bucket.name) }" :aria-label="`${isFavorite(bucket.name) ? 'Remove favorite' : 'Favorite'} ${bucket.name}`" @click="toggleFavorite(bucket.name)">
            <Star :size="13" />
          </button>
        </div>
      </div>
    </div>

    <div class="sidebar-footer">
      <button class="side-action" @click="emit('add')"><Cable :size="17" /> Add connection <Plus class="side-end" :size="15" /></button>
      <button class="side-action" @click="emit('connections')"><Settings2 :size="17" /> Manage connections</button>
    </div>
  </aside>
</template>
