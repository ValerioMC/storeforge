<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import Notice from '../../../components/ui/Notice.vue'
import { errorMessage } from '../../../common/tauri'
import { workspaceSettingsApi } from '../api/workspace-settings'
import { overrideOptions, overrideValue, settingLabel, type OverrideValue, type ResourceKind, type ResourcePolicies } from '../model/workspace-settings'
import QueryPager from './QueryPager.vue'

const props = defineProps<{ serviceId: string; connections?: boolean }>()
const kind = ref<ResourceKind>(props.connections ? 'connection' : 'account')
const kinds: { value: ResourceKind; label: string }[] = [{ value: 'account', label: 'RGW accounts' }, { value: 'user', label: 'RGW users' }]
const inventory = ref<ResourcePolicies>()
const page = ref(1)
const search = ref('')
const appliedSearch = ref('')
const resourceId = ref('')
const pending = ref(false)
const error = ref('')
const options = computed(() => inventory.value?.items.map(item => ({ value: String(item.id), label: item.name })) ?? [])
const selected = computed(() => inventory.value?.items.find(item => String(item.id) === resourceId.value))

onMounted(() => load(1))

function changeKind(value: ResourceKind): void {
  kind.value = value
  search.value = ''
  submitSearch()
}

function submitSearch(): void {
  appliedSearch.value = search.value.trim()
  void load(1)
}

async function load(target: number): Promise<void> {
  pending.value = true
  inventory.value = undefined
  resourceId.value = ''
  error.value = ''
  try {
    inventory.value = await workspaceSettingsApi.resources(props.serviceId, kind.value, target, appliedSearch.value)
    page.value = target
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    pending.value = false
  }
}

async function save(policy: string, value: OverrideValue): Promise<void> {
  const resource = selected.value
  if (!resource || !inventory.value || pending.value) return
  pending.value = true
  error.value = ''
  try {
    const updated = await workspaceSettingsApi.changeResource(props.serviceId, kind.value, resource.id, policy, value === 'inherit' ? null : value === 'allow')
    inventory.value.items = inventory.value.items.map(item => item.id === updated.id ? updated : item)
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    pending.value = false
  }
}
</script>

<template>
  <section class="settings-sheet">
    <div class="settings-row"><h2>{{ connections ? 'Shared connection overrides' : 'Resource policy overrides' }}</h2></div>
    <Notice v-if="error" tone="error">{{ error }}</Notice>
    <div class="settings-row">
      <AppSelect v-if="!connections" :model-value="kind" :options="kinds" aria-label="Resource type" :disabled="pending" @update:model-value="changeKind" />
      <form class="settings-search" @submit.prevent="submitSearch">
        <input v-model="search" type="search" aria-label="Search resources" placeholder="Search resources" />
        <button type="submit" :disabled="pending">Search</button>
      </form>
      <AppSelect v-model="resourceId" :options="options" placeholder="Select resource" aria-label="Policy resource" :disabled="pending" />
      <QueryPager v-if="inventory" :busy="pending" :previous="page > 1" :next="inventory.has_next" @previous="load(page - 1)" @next="load(page + 1)" />
    </div>
    <p v-if="inventory && !inventory.items.length" class="settings-row">No resources match.</p>
    <p v-if="pending" class="settings-row" role="status">Waiting for the service…</p>
    <template v-if="selected">
      <div v-for="(stored, field) in selected.overrides" :key="field" class="settings-row">
        <label :for="`policy-${field}`">{{ settingLabel(field.replace(/^allow_/, '')) }}</label>
        <AppSelect :id="`policy-${field}`" :model-value="overrideValue(stored)" :options="overrideOptions" :disabled="pending" @update:model-value="save(field, $event)" />
      </div>
    </template>
  </section>
</template>

<style scoped>
h2 { font-size: 15px; margin: 0; }
.settings-search { display: flex; flex-wrap: wrap; gap: 6px; }
.settings-row { flex-wrap: wrap; }
</style>
