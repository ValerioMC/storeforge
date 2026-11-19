<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { Bell, ChevronRight, Fingerprint, KeyRound, Server, ShieldCheck } from 'lucide-vue-next'
import { errorMessage } from '../../../common/tauri'
import AppSelect from '../../../components/ui/AppSelect.vue'
import ConfirmDialog from '../../../components/ui/ConfirmDialog.vue'
import EmptyState from '../../../components/ui/EmptyState.vue'
import FadeRegion from '../../../components/ui/FadeRegion.vue'
import Notice from '../../../components/ui/Notice.vue'
import { identityAvailable, providerNames, type Connection, type IdentityKind } from '../../connection/model/connection'
import type { CephGroup } from '../../connection/model/workspace'
import { administer } from '../api/administration'
import { operations, type Operation } from '../model/operations'
import AdminResponse from './AdminResponse.vue'
import IamIdentities from './IamIdentities.vue'
import IamPrivateAccess from './IamPrivateAccess.vue'

const props = defineProps<{ connection?: Connection; bucket?: string; initialGroup?: CephGroup; demo: boolean; scope: 's3' | 'ceph' }>()
const emit = defineEmits<{ open: [profile: Connection] }>()
const group = ref<string>(props.initialGroup ?? 'IAM users')
const selected = ref('')
const values = ref<Record<string, string>>({})
const error = ref('')
const busy = ref(false)
const response = ref<string>()
const confirm = ref(false)
let request = 0

const available = computed(() => operations.filter(operation =>
  (props.scope === 'ceph') === (operation.protocol === 'rgw') &&
  (props.demo || identityAvailable(props.connection, operationIdentity(operation)))))
const groups = computed(() => {
  const names = Array.from(new Set(available.value.map(operation => operation.group)))
  return props.scope === 's3' && !props.demo && names.length
    ? names.map(name => name === 'Access keys' ? 'Personal access' : name)
    : names
})
const groupOperations = computed(() => available.value.filter(operation => operation.group === group.value))
const actionOptions = computed(() => groupOperations.value.map(({ id, label }) => ({ value: id, label })))
const operation = computed(() => groupOperations.value.find(item => item.id === selected.value))
const sampleAvailable = computed(() => props.demo && operation.value?.id === 'ceph-buckets-inspect-bucket-statistics')
const valid = computed(() => !!operation.value && operation.value.fields.every(field => !field.required || values.value[field.key]?.trim()))

watch(() => [props.connection?.id, props.connection?.revision, props.scope], () => {
  if (!groups.value.includes(group.value)) group.value = groups.value[0] ?? ''
  reset()
}, { immediate: true })

watch(groupOperations, items => {
  selected.value = items[0]?.id ?? ''
}, { immediate: true })
watch(selected, reset, { immediate: true })

function operationIdentity(operation: Operation): IdentityKind {
  if (operation.protocol !== 'rgw') return 's3'
  if (operation.group === 'Ceph accounts' || operation.group === 'Ceph users') return 'adminOps'
  if (operation.group === 'Ceph monitoring') return 'supervision'
  return 'cephAdmin'
}

function reset(): void {
  request++
  busy.value = false
  values.value = operation.value?.fields.some(field => field.key === 'bucket') && props.bucket ? { bucket: props.bucket } : {}
  response.value = undefined
  error.value = ''
  confirm.value = false
}

function review(): void {
  if (sampleAvailable.value) {
    response.value = JSON.stringify({
      bucket: props.bucket || 'mission-archive', owner: 'asteria-observatory', tenant: 'science',
      placement_rule: 'default-placement', index_type: 'Normal', num_shards: 11,
      usage: { 'rgw.main': { num_objects: 86420, size_actual: 9843511104 } },
      bucket_quota: { enabled: true, max_size: 53687091200, max_objects: 250000 }
    })
    return
  }
  if (operation.value?.write) confirm.value = true
  else void execute(false)
}

async function execute(confirmed: boolean): Promise<void> {
  const selectedOperation = operation.value
  if (!selectedOperation || !props.connection || !valid.value || busy.value) return
  const id = ++request
  confirm.value = false
  busy.value = true
  response.value = undefined
  error.value = ''
  try {
    const result = await administer(props.connection.id, { operation: selectedOperation.id, values: values.value, confirmed })
    if (id === request) response.value = result
  } catch (failure) {
    if (id === request) error.value = errorMessage(failure)
  } finally {
    if (id === request) busy.value = false
  }
}
</script>

<template>
  <section class="workspace-page">
    <div class="page-header">
      <div><h1>{{ scope === 'ceph' ? 'Ceph administration' : 'Access & topics' }}</h1><p>{{ scope === 'ceph' ? 'Manage RGW resources with the profile’s verified service identities.' : 'Manage IAM access and notification topics.' }}</p></div>
      <span v-if="connection" class="context-chip"><Server :size="16" />{{ connection.name }} · {{ providerNames[connection.provider] }}</span>
    </div>
    <EmptyState v-if="!connection" :icon="ShieldCheck" title="Select a connection" description="Administrative actions use the permissions of the selected connection." />
    <template v-else>
      <Notice v-if="demo">Preview the administration forms. Ceph buckets includes a sample statistics response.</Notice>
      <Notice v-if="!groups.length">This profile has no available administration identity. Check its credentials in Connections.</Notice>
      <div v-else class="stage-panel admin-grid">
        <nav class="settings-nav" aria-label="Administration sections">
          <button v-for="name in groups" :key="name" :class="{ active: name === group }" :aria-pressed="name === group" @click="group = name">
            <Server v-if="name.startsWith('Ceph')" :size="15" /><Bell v-else-if="name === 'Notifications'" :size="15" /><KeyRound v-else-if="name === 'Access keys'" :size="15" /><Fingerprint v-else :size="15" />{{ name }}
            <ChevronRight :size="14" class="side-end" />
          </button>
        </nav>
        <div class="admin-body">
          <FadeRegion>
            <div :key="group">
              <IamIdentities v-if="!demo && connection && (group === 'IAM users' || group === 'IAM groups')"
                :connection="connection.id" :kind="group === 'IAM users' ? 'User' : 'Group'" @personal-access="group = 'Personal access'" />
              <IamPrivateAccess v-else-if="!demo && connection && group === 'Personal access'" :profile="connection" @open="emit('open', $event)" />
              <form v-else class="panel-form" @submit.prevent="review">
                <div class="stage-heading">
                  <div><h2>{{ group }}</h2><p>{{ operation?.protocol === 'rgw' ? 'Manage the selected gateway using its RGW Admin Ops API.' : 'Manage resources within the selected account.' }}</p></div>
                  <span class="chip">{{ operation?.protocol.toUpperCase() }}</span>
                </div>
                <label>Action<AppSelect v-model="selected" :options="actionOptions" :disabled="busy" /></label>
                <FadeRegion>
                  <div :key="selected" class="panel-form">
                    <div v-if="operation?.fields.length" class="form-grid">
                      <label v-for="field in operation.fields" :key="field.key" :class="{ 'span-two': field.multiline }">
                        {{ field.label }} <small v-if="!field.required">Optional</small>
                        <textarea v-if="field.multiline" v-model="values[field.key]" :required="field.required" rows="7" spellcheck="false" />
                        <input v-else v-model="values[field.key]" :required="field.required" spellcheck="false" />
                      </label>
                    </div>
                    <Notice v-if="error" tone="error">{{ error }}</Notice>
                    <div class="panel-actions"><button class="primary" :disabled="busy || !valid || (demo && !sampleAvailable)">{{ sampleAvailable ? 'Show sample response' : busy ? 'Waiting for endpoint…' : operation?.write ? 'Review change' : 'Run request' }}</button></div>
                  </div>
                </FadeRegion>
              </form>
              <AdminResponse v-if="response !== undefined" :key="request" :response="response" :demo="demo" />
            </div>
          </FadeRegion>
        </div>
      </div>
    </template>
    <ConfirmDialog v-if="confirm && operation" :title="`${operation.label}?`"
      :description="`This changes remote resources on ${connection?.name}. Review the selected action and fields before applying it.`"
      :action="operation.label" :danger="operation.label.startsWith('Delete') || operation.label.startsWith('Repair')"
      @close="confirm = false" @confirm="execute(true)" />
  </section>
</template>
