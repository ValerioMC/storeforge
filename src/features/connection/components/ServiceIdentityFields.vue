<script setup lang="ts">
import { computed, reactive, watch } from 'vue'
import { capabilityNames, identityNames, type ServiceDraft, type ServiceIdentity, type ServiceIdentityKind } from '../model/connection'

const props = defineProps<{ identity: ServiceIdentityKind; saved?: ServiceIdentity; available: boolean }>()
const change = defineModel<ServiceDraft | null>({ required: true })
const fields = reactive({
  selected: false, enabled: props.saved?.enabled ?? true, endpoint: props.saved?.endpoint ?? '',
  accessKey: '', secretKey: '', sessionToken: '',
})
const editable = computed(() => !props.saved?.managedExternally && (props.available || props.saved?.enabled))
const status = computed(() => props.available && props.saved?.enabled ? props.saved.capability : 'unavailable')

watch(fields, () => {
  change.value = fields.selected ? {
    enabled: fields.enabled,
    endpoint: fields.endpoint,
    credentials: fields.accessKey || fields.secretKey || fields.sessionToken ?
      { accessKey: fields.accessKey.trim(), secretKey: fields.secretKey, sessionToken: fields.sessionToken || null } : null,
  } : null
}, { flush: 'sync' })

watch(editable, value => { if (!value) fields.selected = false })
watch(() => props.available, value => { if (!value) fields.enabled = false })
</script>

<template>
  <div class="disclosure">
    <div class="inline between">
      <strong>{{ identityNames[identity] }}</strong>
      <span class="chip">{{ capabilityNames[status] }}</span>
    </div>
    <p v-if="saved?.managedExternally" class="muted">Managed externally</p>
    <template v-else-if="editable">
      <label class="check-row"><input v-model="fields.selected" type="checkbox" /> Reconcile this identity</label>
      <div v-if="fields.selected" class="form-grid">
        <label class="check-row span-two"><input v-model="fields.enabled" type="checkbox" :disabled="!available" /> Enable {{ identityNames[identity] }}</label>
        <template v-if="fields.enabled">
          <label class="span-two">Service URL <small>Optional</small><input v-model="fields.endpoint" type="url" placeholder="Use the profile endpoint" /></label>
          <label>Access key ID<input v-model="fields.accessKey" autocomplete="off" spellcheck="false" /></label>
          <label>Secret access key<input v-model="fields.secretKey" type="password" autocomplete="off" /></label>
          <label class="span-two">Session token <small>Optional</small><input v-model="fields.sessionToken" type="password" autocomplete="off" /></label>
        </template>
      </div>
    </template>
  </div>
</template>
