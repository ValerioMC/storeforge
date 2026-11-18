<script setup lang="ts">
import { computed, ref } from 'vue'
import { Star, Trash2 } from 'lucide-vue-next'
import { workspaceAvailable } from '../../connection/model/workspace'
import { useConnections } from '../../connection/store/connections'
import { browserLocationKey } from '../model/listing'
import type { Location } from '../model/objects'
import { matchesFavorite } from '../model/preferences'

const props = defineProps<{ favorites: Location[] }>()
const emit = defineEmits<{ open: [location: Location]; remove: [location: Location] }>()
const connections = useConnections()
const query = ref('')
const shown = computed(() => props.favorites.map(location => ({
  location,
  profile: location.connection === 'demo' ? 'Asteria Research (demo)' : connections.profiles.find(item => item.id === location.connection)?.name ?? 'Unavailable connection',
  available: location.connection === 'demo' || workspaceAvailable(connections.profiles.find(item => item.id === location.connection), 'browser'),
})).filter(item => matchesFavorite(item.location, item.profile, query.value)))
</script>

<template>
  <details class="browser-favorites">
    <summary><Star :size="15" /> Favorites</summary>
    <div class="favorites-panel">
      <input v-model="query" type="search" aria-label="Search favorites" placeholder="Find by connection, bucket or prefix" />
      <p v-if="!shown.length" class="muted">{{ favorites.length ? 'No matching favorites' : 'No saved paths' }}</p>
      <ul v-else>
        <li v-for="item in shown" :key="browserLocationKey(item.location)">
          <button class="favorite-target" :disabled="!item.available" @click="emit('open', item.location)">
            <span>{{ item.profile }}<template v-if="!item.available && item.profile !== 'Unavailable connection'"> · Browser unavailable</template></span>
            <code>s3://{{ item.location.bucket }}/{{ item.location.prefix }}</code>
          </button>
          <button class="icon-button subtle" :aria-label="'Remove favorite ' + item.location.bucket + '/' + item.location.prefix" @click="emit('remove', item.location)"><Trash2 :size="15" /></button>
        </li>
      </ul>
    </div>
  </details>
</template>

<style scoped>
summary { display: flex; align-items: center; gap: 6px; width: max-content; cursor: pointer; }
.favorites-panel { display: grid; gap: 8px; margin-top: 7px; padding: 8px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--surface); }
ul { list-style: none; padding: 0; margin: 0; max-height: 180px; overflow: auto; }
li { display: flex; align-items: center; gap: 5px; }
.favorite-target { display: grid; justify-items: start; flex: 1; gap: 3px; min-width: 0; }
.favorite-target code { max-width: 100%; text-overflow: ellipsis; overflow: hidden; white-space: pre; }
</style>
