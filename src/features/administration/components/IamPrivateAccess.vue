<script setup lang="ts">
import { onScopeDispose, ref, watch } from 'vue'
import { Plus, RefreshCw } from 'lucide-vue-next'
import { errorMessage } from '../../../common/tauri'
import ConfirmDialog from '../../../components/ui/ConfirmDialog.vue'
import Notice from '../../../components/ui/Notice.vue'
import type { Connection } from '../../connection/model/connection'
import { useConnections } from '../../connection/store/connections'
import { changePrivateAccess, listPrivateAccess } from '../api/iam'
import type { PrivateAccess, PrivateAccessChange, PrivateAccessDraft } from '../model/iam'
import IamIdentityEditor from './IamIdentityEditor.vue'

const props = defineProps<{ profile: Connection }>()
const emit = defineEmits<{ open: [profile: Connection] }>()
const connections = useConnections()
const records = ref<PrivateAccess[]>([])
const busy = ref(false)
const loading = ref(false)
const error = ref('')
const createError = ref('')
const creating = ref(false)
const retiring = ref<PrivateAccess>()
const states: Record<PrivateAccess['status'], string> = {
  active: 'Active', provisioning: 'Setup interrupted', setupIncomplete: 'Setup incomplete', retiring: 'Retirement incomplete',
}
let generation = 0
onScopeDispose(() => { generation++ })
watch(() => props.profile.id, refresh, { immediate: true })

async function refresh(): Promise<void> {
  const current = ++generation
  loading.value = true
  error.value = ''
  try {
    const result = await listPrivateAccess(props.profile.id)
    if (current === generation) records.value = result
  } catch (failure) {
    if (current === generation) error.value = errorMessage(failure)
  } finally {
    if (current === generation) loading.value = false
  }
}

async function apply(change: PrivateAccessChange): Promise<void> {
  if (busy.value) return
  const current = generation
  busy.value = true
  error.value = ''
  createError.value = ''
  retiring.value = undefined
  try {
    const result = await changePrivateAccess(props.profile.id, change)
    if (change.action !== 'create' || result?.status === 'active') {
      await connections.refreshProfile(result?.id ?? (change.action === 'create' ? '' : change.id))
    }
    if (current === generation) creating.value = false
  } catch (failure) {
    if (current === generation) {
      if (creating.value) createError.value = errorMessage(failure)
      else error.value = errorMessage(failure)
    }
  } finally {
    if (current === generation) {
      const issue = error.value
      await refresh()
      error.value = issue || error.value
      busy.value = false
    }
  }
}

function create(draft: PrivateAccessDraft): void {
  void apply({ action: 'create', draft })
}

async function open(id: string): Promise<void> {
  busy.value = true
  error.value = ''
  try {
    const profile = await connections.refreshProfile(id)
    if (!profile) throw new Error('The saved connection could not be found.')
    emit('open', profile)
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div class="panel-form">
    <div class="stage-heading">
      <div><h2>Personal access</h2><p>Open or retire private connections provisioned from this IAM account.</p></div>
      <button v-if="!profile.settings?.managedIam && !profile.settings?.openstack" class="primary" :disabled="busy" @click="creating = true"><Plus :size="16" /> Provision connection</button>
    </div>
    <div class="panel-actions"><button :disabled="busy || loading" @click="refresh"><RefreshCw :size="15" /> Refresh</button></div>
    <Notice v-if="profile.settings?.openstack">Use a direct administrator connection to provision IAM access.</Notice>
    <Notice v-else-if="profile.settings?.managedIam">Use the administrator connection to manage this profile’s IAM identity.</Notice>
    <Notice v-if="error" tone="error">{{ error }}</Notice>
    <p v-else-if="loading" role="status">Loading private connections…</p>
    <p v-else-if="!records.length" class="muted">No private connections provisioned from this profile.</p>
    <div v-for="record in records" :key="record.id" class="settings-row">
      <div class="settings-description"><strong>{{ record.name }}</strong><p>{{ record.userName }}</p><Notice v-if="record.issue" tone="error">{{ record.issue }}</Notice></div>
      <span class="chip">{{ states[record.status] }}</span>
      <button v-if="record.status === 'active'" :disabled="busy" @click="open(record.id)">Open connection</button>
      <button v-else :disabled="busy" @click="apply({ action: 'retry', id: record.id })">{{ record.status === 'retiring' ? 'Retry retirement' : 'Retry setup' }}</button>
      <button v-if="record.status !== 'retiring'" :disabled="busy" @click="retiring = record">Retire</button>
    </div>
    <IamIdentityEditor v-if="creating" :connection="profile.id" kind="User" private-access :busy="busy" :error="createError"
      @close="creating = false; createError = ''" @save="create" />
    <ConfirmDialog v-if="retiring" :title="`Retire ${retiring.name}?`" description="This revokes the dedicated IAM user’s credentials and removes the saved connection."
      action="Retire connection" danger @close="retiring = undefined" @confirm="apply({ action: 'retire', id: retiring.id })" />
  </div>
</template>
