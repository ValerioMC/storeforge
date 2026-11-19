<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import Notice from '../../../components/ui/Notice.vue'
import { errorMessage } from '../../../common/tauri'
import { workspaceSettingsApi } from '../api/workspace-settings'
import { featurePolicyOptions, settingLabel, type PolicySettings, type SettingEdit, type SettingsSection, type SettingValue } from '../model/workspace-settings'

const props = defineProps<{ serviceId: string; section: SettingsSection }>()
const saved = ref<PolicySettings>()
const draft = ref<PolicySettings['groups']>([])
const pending = ref(false)
const error = ref('')
const success = ref(false)
const edits = computed(() => draft.value.flatMap((group, index) => {
  const previous = saved.value?.groups[index]
  if (!previous) return []
  return Object.entries(group.values).flatMap<SettingEdit>(([field, value]) => {
    const before = previous.values[field]
    return before === undefined || before === value ? [] : [{ group: group.name, field, before, value }]
  })
}))

onMounted(() => load())

function change(group: PolicySettings['groups'][number], key: string, value: SettingValue): void {
  group.values[key] = value
  success.value = false
}

function changeNumber(group: PolicySettings['groups'][number], key: string, event: Event): void {
  const value = (event.target as HTMLInputElement).valueAsNumber
  if (Number.isSafeInteger(value) && value >= 0) change(group, key, value)
}

async function load(changes?: SettingEdit[]): Promise<void> {
  pending.value = true
  error.value = ''
  success.value = false
  try {
    saved.value = await workspaceSettingsApi.settings(props.serviceId, props.section, changes)
    draft.value = saved.value.groups.map(group => ({ name: group.name, values: { ...group.values } }))
    success.value = !!changes
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    pending.value = false
  }
}
</script>

<template>
  <form class="policy-form" @submit.prevent="load(edits)">
    <Notice v-if="error" tone="error">{{ error }}</Notice>
    <Notice v-if="success" tone="success">Settings saved.</Notice>
    <p v-if="pending && !saved" role="status">Reading settings…</p>
    <template v-if="saved">
      <section v-for="group in draft.filter(group => Object.keys(group.values).length)" :key="group.name ?? 'main'" class="settings-sheet">
        <h2 v-if="group.name" class="settings-row">{{ settingLabel(group.name) }}</h2>
        <div v-for="(value, key) in group.values" :key="key" class="settings-row">
          <label :for="`setting-${group.name}-${key}`" class="settings-description">
            <strong>{{ settingLabel(key) }}</strong>
            <p v-if="group.name === null && saved.locks[key]?.forced">Locked by {{ saved.locks[key].source || 'deployment configuration' }}</p>
          </label>
          <input v-if="typeof value === 'boolean'" :id="`setting-${group.name}-${key}`" type="checkbox" :checked="value"
            :disabled="pending || (group.name === null && saved.locks[key]?.forced)"
            @change="change(group, key, ($event.target as HTMLInputElement).checked)" />
          <input v-else-if="typeof value === 'number'" :id="`setting-${group.name}-${key}`" type="number" min="0" required :value="value"
            :disabled="pending" @input="changeNumber(group, key, $event)" />
          <AppSelect v-else :id="`setting-${group.name}-${key}`" :model-value="value" :options="featurePolicyOptions" :disabled="pending"
            @update:model-value="change(group, key, $event)" />
        </div>
      </section>
      <div class="settings-toolbar"><button type="submit" class="primary" :disabled="pending || !edits.length">Save policies</button></div>
    </template>
  </form>
</template>

<style scoped>
.policy-form { display: grid; gap: 10px; }
h2 { font-size: 15px; margin: 0; }
input[type=number] { width: 100px; }
</style>
