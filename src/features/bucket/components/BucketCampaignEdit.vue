<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import { campaignSections, exampleDocument, isRuleSection, type CampaignConfiguration, type CampaignSection } from '../model/campaign'

export interface CampaignEdit {
  section: CampaignSection
  operation: 'merge' | 'replace' | 'remove'
  document: string
  removeIds: string
  versioning: 'Enabled' | 'Suspended'
  setQuotaEnabled: boolean
  setMaxBytes: boolean
  setMaxObjects: boolean
  quotaEnabled: boolean
  maxBytes: string
  maxObjects: string
}

const edit = defineModel<CampaignEdit>({ required: true })
const operations = computed(() => {
  const options: { value: CampaignEdit['operation']; label: string }[] = []
  if (ruleSection.value) options.push({ value: 'merge', label: 'Add, update or remove rules by ID' })
  return [...options, { value: 'replace', label: 'Replace this setting' }, { value: 'remove', label: 'Remove this setting' }]
})
const versioningStates = [{ value: 'Enabled', label: 'Enabled' }, { value: 'Suspended', label: 'Suspended' }] as const
const ruleSection = computed(() => edit.value.section !== 'quota' && isRuleSection(edit.value.section))
const examples = ref<Partial<Record<CampaignConfiguration, string>>>({})

watch(() => edit.value.section, (section, previous) => {
  if (previous !== 'quota') examples.value[previous] = edit.value.document
  edit.value = { ...edit.value, operation: ruleSection.value ? 'merge' : 'replace',
    document: section === 'quota' ? '' : examples.value[section] ?? '', removeIds: '' }
})

function addExample(): void {
  if (edit.value.section !== 'quota') edit.value.document = exampleDocument(edit.value.section)
}
</script>

<template>
  <div class="panel-form">
    <label>Setting <AppSelect v-model="edit.section" :options="campaignSections" aria-label="Setting" /></label>
    <template v-if="edit.section === 'quota'">
      <div class="quota-fields">
        <label class="check-label"><input v-model="edit.setQuotaEnabled" type="checkbox" /> Update enforcement</label>
        <label class="check-label"><input v-model="edit.quotaEnabled" type="checkbox" :disabled="!edit.setQuotaEnabled" /> Enforce quota</label>
        <label class="check-label"><input v-model="edit.setMaxBytes" type="checkbox" /> Update storage limit</label>
        <input v-model="edit.maxBytes" inputmode="numeric" aria-label="Storage limit in bytes" :disabled="!edit.setMaxBytes" placeholder="Bytes or blank for unlimited" />
        <label class="check-label"><input v-model="edit.setMaxObjects" type="checkbox" /> Update object limit</label>
        <input v-model="edit.maxObjects" inputmode="numeric" aria-label="Object limit" :disabled="!edit.setMaxObjects" placeholder="Count or blank for unlimited" />
      </div>
    </template>
    <label v-else-if="edit.section === 'versioning'">Versioning status <AppSelect v-model="edit.versioning" :options="versioningStates" aria-label="Versioning status" /></label>
    <template v-else>
      <label>Change <AppSelect v-model="edit.operation" :options="operations" aria-label="Configuration change" /></label>
      <template v-if="edit.operation !== 'remove'">
        <label>{{ edit.section === 'policy' ? 'JSON policy' : 'XML document' }}
          <textarea v-model="edit.document" rows="7" spellcheck="false" aria-label="Configuration document" />
        </label>
        <div class="panel-actions"><button type="button" @click="addExample">Load example</button></div>
      </template>
      <label v-if="ruleSection && edit.operation === 'merge'">Rule IDs to remove
        <textarea v-model="edit.removeIds" rows="2" placeholder="One rule ID per line" />
      </label>
    </template>
  </div>
</template>

<style scoped>
.quota-fields { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); align-items: center; gap: 9px 12px; }
textarea { width: 100%; font-family: var(--mono); }
</style>
