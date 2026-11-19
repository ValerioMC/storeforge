<script setup lang="ts">
import { ref, watch } from 'vue'
import Notice from '../../../components/ui/Notice.vue'
import { portalApi } from '../api/portal'
import { usePortalTask } from '../composables/usePortalTask'
import type { Candidate, Page, PortalScope, SpaceScope } from '../model/spaces'

const props = defineProps<{ scope: PortalScope | SpaceScope; selected: readonly number[] }>()
const emit = defineEmits<{ pick: [candidate: Candidate] }>()
const search = ref('')
const page = ref(1)
const { data, error, pending, run, clear } = usePortalTask<Page<Candidate>>()
watch([search, page], ([query, current], _, onCleanup) => {
  clear()
  const timer = window.setTimeout(() => void run(() => portalApi.candidates(props.scope, query.trim(), current)), query ? 180 : 0)
  onCleanup(() => window.clearTimeout(timer))
}, { immediate: true })

function filter(event: Event): void {
  search.value = (event.target as HTMLInputElement).value
  page.value = 1
}
</script>

<template>
  <div class="portal-candidates">
    <label class="portal-control">Find people <input type="search" :value="search" @input="filter" /></label>
    <Notice v-if="error" tone="error">{{ error }}</Notice>
    <p v-else-if="pending" class="muted portal-note" role="status">Searching…</p>
    <ul v-else-if="data?.items.length" class="portal-list">
      <li v-for="person in data.items" :key="person.user_id" class="portal-between">
        <span>{{ person.display_name || person.email }} <small v-if="person.display_name" class="muted">{{ person.email }}</small></span>
        <button type="button" :disabled="selected.includes(person.user_id) || person.already_shared" @click="emit('pick', person)">
          {{ selected.includes(person.user_id) || person.already_shared ? 'Selected' : 'Select' }}
        </button>
      </li>
    </ul>
    <p v-else-if="data" class="muted portal-note">No matching people.</p>
    <div v-if="data && data.total_pages > 1" class="portal-inline">
      <button type="button" :disabled="page <= 1" @click="page--">Previous</button>
      <span>{{ page }} / {{ data.total_pages }}</span>
      <button type="button" :disabled="page >= data.total_pages" @click="page++">Next</button>
    </div>
  </div>
</template>

<style scoped>
.portal-candidates { display: grid; gap: 8px; }
.portal-list { max-height: 180px; overflow: auto; }
</style>
