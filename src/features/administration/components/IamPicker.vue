<script setup lang="ts">
import { computed, ref } from 'vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import Notice from '../../../components/ui/Notice.vue'
import { listIdentities, listPolicies } from '../api/iam'
import { useQueryPage } from '../composables/useQueryPage'
import { managedUser, type QueryPage } from '../model/iam'
import QueryPager from './QueryPager.vue'

interface Candidate { value: string; label: string }
const props = defineProps<{ connection: string; kind: 'User' | 'Group' | 'Policy'; label: string; disabled?: boolean }>()
const chosen = defineModel<string[]>({ required: true })
const selected = ref('')
const labels = ref<Record<string, string>>({})
const { items, markers, nextMarker, loading, error, search, next, previous, reload } = useQueryPage(
  () => [props.connection, props.kind], candidates, item => item.label,
)
const options = computed(() => items.value.filter(option => !chosen.value.includes(option.value)))

async function candidates(marker?: string): Promise<QueryPage<Candidate>> {
  if (props.kind === 'Policy') {
    const page = await listPolicies(props.connection, marker)
    return { ...page, items: page.items.map(policy => ({ value: policy.arn, label: policy.name })) }
  }
  const page = await listIdentities(props.connection, props.kind, marker)
  return { ...page, items: page.items.filter(identity => !managedUser(identity.name) || props.kind !== "User").map(identity => ({ value: identity.name, label: identity.name })) }
}

function add(): void {
  const option = options.value.find(option => option.value === selected.value)
  if (!option) return
  chosen.value = [...chosen.value, option.value]
  labels.value[option.value] = option.label
  selected.value = ''
}
</script>

<template>
  <fieldset class="panel-form iam-picker" :disabled="disabled">
    <legend>{{ label }}</legend>
    <div class="form-grid">
      <label>Search {{ kind.toLowerCase() }}s
        <input v-model="search" type="search" @keydown.enter.prevent="reload" />
      </label>
      <button type="button" :disabled="loading" @click="reload">Search</button>
      <label>{{ kind }}
        <AppSelect v-model="selected" :options="options" :disabled="disabled || loading"
          :placeholder="loading ? 'Loading…' : options.length ? 'Select an option' : 'No available matches'" />
      </label>
      <button type="button" :disabled="loading || !options.some(option => option.value === selected)" @click="add">Add {{ kind.toLowerCase() }}</button>
    </div>
    <Notice v-if="error" tone="error">{{ error }}</Notice>
    <QueryPager :busy="loading" :previous="!!markers.length" :next="!!nextMarker" @previous="previous" @next="next" />
    <div v-if="chosen.length" class="connection-tags">
      <button v-for="value in chosen" :key="value" type="button" class="chip"
        :aria-label="`Remove selection ${labels[value] ?? value}`" @click="chosen = chosen.filter(item => item !== value)">{{ labels[value] ?? value }} ×</button>
    </div>
  </fieldset>
</template>

<style scoped>
.iam-picker { min-width: 0; border: 1px solid var(--line); border-radius: 8px; padding: 12px; }
.iam-picker .form-grid { align-items: end; }
.iam-picker .form-grid > button { justify-self: start; }
</style>
