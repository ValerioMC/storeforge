<script setup lang="ts">
import { computed, reactive } from 'vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import { bulkAttributes, emptyAttributeDraft, metadataFields, type BulkAttributes } from '../model/batch'

defineProps<{ objects: number; folders: number; busy: boolean; path: string; error: string }>()
const emit = defineEmits<{ close: []; apply: [update: BulkAttributes] }>()
const draft = reactive(emptyAttributeDraft())
const storageClasses = ['STANDARD', 'REDUCED_REDUNDANCY', 'STANDARD_IA', 'ONEZONE_IA', 'INTELLIGENT_TIERING', 'GLACIER_IR', 'GLACIER', 'DEEP_ARCHIVE']
const aclOptions = ['private', 'public-read', 'public-read-write', 'authenticated-read', 'bucket-owner-read', 'bucket-owner-full-control']
const retentionModes = [{ value: '' as const, label: 'Select mode' }, { value: 'GOVERNANCE' as const, label: 'Governance' }, { value: 'COMPLIANCE' as const, label: 'Compliance' }]
const holdOptions = [{ value: '' as const, label: 'Leave unchanged' }, { value: 'on' as const, label: 'On' }, { value: 'off' as const, label: 'Off' }]
const validation = computed(() => {
  try {
    return { update: bulkAttributes(draft), error: '' }
  } catch (failure) {
    return { error: failure instanceof Error ? failure.message : 'Review the selected attributes.' }
  }
})

function apply(): void {
  if (validation.value.update) emit('apply', validation.value.update)
}
</script>

<template>
  <Modal title="Bulk attributes" wide @close="emit('close')">
    <form class="modal-form batch-form" @submit.prevent="apply">
      <p><code class="batch-path">{{ path }}</code> · {{ objects }} {{ objects === 1 ? 'file' : 'files' }} · {{ folders }} {{ folders === 1 ? 'folder' : 'folders' }}</p>
      <p v-if="folders" class="muted">Folders include all nested objects.</p>
      <fieldset :disabled="busy">
        <legend>Metadata headers</legend>
        <label class="check"><input v-model="draft.metadataEnabled" type="checkbox" /> Replace metadata and headers</label>
        <template v-if="draft.metadataEnabled">
          <div class="attribute-fields">
            <label v-for="{ field, label } in metadataFields" :key="field">{{ label }}
              <input v-model="draft.headers[field]" :type="field === 'expires' ? 'datetime-local' : 'text'" />
            </label>
          </div>
          <label>Custom metadata<textarea v-model="draft.metadata" rows="3" placeholder="key=value" /></label>
          <small class="muted">Blank fields clear existing values. Enter one pair per line.</small>
        </template>
      </fieldset>
      <fieldset :disabled="busy">
        <legend>Tags</legend>
        <label class="check"><input v-model="draft.tagsEnabled" type="checkbox" /> Replace tags</label>
        <label v-if="draft.tagsEnabled">Tag entries<textarea v-model="draft.tags" rows="3" placeholder="key=value" />
          <small class="muted">One pair per line. Leave empty to remove all tags.</small>
        </label>
      </fieldset>
      <div class="attribute-fields">
        <label>Storage class<AppSelect v-model="draft.storageClass" :disabled="busy"
          :options="[{ value: '', label: 'Leave unchanged' }, ...storageClasses.map(value => ({ value, label: value }))]" /></label>
        <label>Access control list<AppSelect v-model="draft.acl" :disabled="busy"
          :options="[{ value: '', label: 'Leave unchanged' }, ...aclOptions.map(value => ({ value, label: value }))]" /></label>
        <label>Legal hold<AppSelect v-model="draft.legalHold" :disabled="busy" :options="holdOptions" /></label>
      </div>
      <fieldset :disabled="busy">
        <legend>Retention</legend>
        <label class="check"><input v-model="draft.retentionEnabled" type="checkbox" /> Update retention</label>
        <template v-if="draft.retentionEnabled">
          <div class="attribute-fields">
            <label>Mode<AppSelect v-model="draft.retentionMode" :options="retentionModes" /></label>
            <label>Retain until<input v-model="draft.retainUntil" type="datetime-local" required /></label>
          </div>
          <label class="check"><input v-model="draft.bypassGovernance" type="checkbox" /> Bypass governance retention when authorized</label>
        </template>
      </fieldset>
      <Notice v-if="error" tone="error">{{ error }}</Notice>
      <p v-if="validation.error" class="muted" role="status">{{ validation.error }}</p>
      <footer class="modal-actions"><button type="button" @click="emit('close')">Cancel</button>
        <button class="primary" :disabled="busy || !validation.update">Apply changes</button>
      </footer>
    </form>
  </Modal>
</template>

<style scoped>
.batch-form fieldset { min-inline-size: 0; border: 1px solid var(--line); border-radius: var(--radius); padding: 10px 12px; margin: 0; }
.batch-form label.check { display: flex; align-items: center; gap: 7px; }
.batch-form fieldset label + label, .batch-form fieldset div + label { margin-top: 8px; }
.attribute-fields { display: grid; gap: 8px; grid-template-columns: repeat(auto-fit, minmax(min(100%, 180px), 1fr)); }
.batch-path { white-space: pre-wrap; overflow-wrap: anywhere; }
</style>
