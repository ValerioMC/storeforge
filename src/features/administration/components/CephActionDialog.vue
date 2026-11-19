<script setup lang="ts">
import { computed, ref } from 'vue'
import { errorMessage } from '../../../common/tauri'
import AppSelect from '../../../components/ui/AppSelect.vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import type { WorkbenchRow } from '../../bucket/model/workbench'
import { identityAvailable, type Connection } from '../../connection/model/connection'
import { cephAction, resolveCephOwner } from '../api/ceph-operations'
import { cephBucketLabel, cephConfirmation, ownerLookup, type BucketOwner, type CephAction, type CephOutcome } from '../model/ceph-operations'
import { rgwTargetLabel, rgwUserLabel, type RgwTarget } from '../model/rgw'
import CephActionResult from './CephActionResult.vue'
import RgwIdentityPicker from './RgwIdentityPicker.vue'

type Selection =
  | { kind: 'delete' | 'link' | 'unlink' | 'index'; row: WorkbenchRow }
  | { kind: 'deleteUser'; identity: Extract<CephAction, { kind: 'deleteUser' }>['identity']; account: string | null }
  | { kind: 'deleteAccount'; id: string; tenant: string }

const props = defineProps<{ profile: Connection; selection: Selection }>()
const emit = defineEmits<{ close: [changed: boolean] }>()
const purge = ref(false)
const bypass = ref(false)
const repair = ref(false)
const checkObjects = ref(false)
const ownerKind = ref<'user' | 'account'>('user')
const ownerId = ref('')
const ownerTenant = ref('')
const destination = ref<BucketOwner>()
const confirmation = ref('')
const result = ref<CephOutcome>()
const busy = ref(false)
const failure = ref('')
const ownerKinds = [{ value: 'user', label: 'RGW user' }, { value: 'account', label: 'RGW account' }] as const

const row = computed(() => 'row' in props.selection ? props.selection.row : undefined)
const title = computed(() => {
  switch (props.selection.kind) {
    case 'delete': return 'Delete bucket'
    case 'link': return 'Link bucket'
    case 'unlink': return 'Unlink bucket'
    case 'index': return 'Check bucket index'
    case 'deleteUser': return 'Delete RGW user'
    case 'deleteAccount': return 'Delete RGW account'
  }
})
const target = computed(() => 'row' in props.selection ? cephBucketLabel(props.selection.row.target) :
  props.selection.kind === 'deleteUser' ? rgwUserLabel(props.selection.identity) : props.selection.id)
const tenant = computed(() => 'row' in props.selection ? props.selection.row.tenant :
  props.selection.kind === 'deleteUser' ? props.selection.identity.tenant : props.selection.tenant)
const endpoint = computed(() => props.profile.services?.[row.value ? 'cephAdmin' : 'adminOps']?.endpoint || props.profile.endpoint)
const action = computed<CephAction | undefined>(() => {
  const selection = props.selection
  if (selection.kind === 'deleteUser') return { ...selection, purgeData: purge.value }
  if (selection.kind === 'deleteAccount') return selection
  const bucket = { kind: 'bucket' as const, target: selection.row.target, owner: selection.row.owner }
  switch (selection.kind) {
    case 'delete': return { ...bucket, change: { kind: 'delete', purgeObjects: purge.value, bypassGc: purge.value && bypass.value } }
    case 'link': return destination.value ? { ...bucket, change: { kind: 'link', destination: destination.value } } : undefined
    case 'unlink': return { ...bucket, change: { kind: 'unlink' } }
    case 'index': return { ...bucket, change: { kind: 'index', repair: repair.value, checkObjects: repair.value && checkObjects.value } }
  }
})
const phrase = computed(() => action.value ? cephConfirmation(action.value) : '')
const impact = computed(() => {
  switch (props.selection.kind) {
    case 'delete': return purge.value
      ? 'Permanently remove the bucket and its objects.' + (bypass.value ? ' Object removal bypasses the gateway’s normal garbage collection queue.' : '')
      : 'Request deletion of the empty bucket. RGW rejects a bucket that contains objects.'
    case 'link': return 'Changes which user or account owns this bucket. Access through the current owner may change.'
    case 'unlink': return 'Removes the current owner’s bucket association. The bucket stays on the gateway.'
    case 'index': return repair.value ? 'RGW can rewrite bucket index entries.' : 'Read index diagnostics for this bucket.'
    case 'deleteUser': return purge.value ? 'Permanently remove the user, their keys and their stored data.' : 'Remove the user and their keys. RGW can reject deletion while the user still owns buckets.'
    case 'deleteAccount': return 'Remove the account. RGW can reject deletion while it still owns resources.'
  }
})

function clearDestination(): void {
  destination.value = undefined
  confirmation.value = ''
  failure.value = ''
}

async function lookup(owner: RgwTarget): Promise<void> {
  clearDestination()
  busy.value = true
  try {
    destination.value = await resolveCephOwner(props.profile, owner)
  } catch (error) {
    failure.value = errorMessage(error)
  } finally {
    busy.value = false
  }
}

async function run(): Promise<void> {
  if (!action.value || busy.value || phrase.value !== confirmation.value) return
  busy.value = true
  failure.value = ''
  result.value = undefined
  try {
    result.value = await cephAction(props.profile, { ...action.value, confirmation: confirmation.value })
  } catch (error) {
    failure.value = errorMessage(error)
  } finally {
    busy.value = false
  }
}

function close(): void {
  if (!busy.value) emit('close', !!result.value?.success && !!phrase.value)
}
</script>

<template>
  <Modal :title="title" wide @close="close">
    <form class="modal-form" @submit.prevent="run">
      <dl class="setup-summary">
        <div><dt>Endpoint</dt><dd>{{ profile.name }} · {{ endpoint }}</dd></div>
        <div><dt>Tenant</dt><dd>{{ tenant || 'Default' }}</dd></div>
        <div><dt>Target</dt><dd>{{ target }}</dd></div>
        <div v-if="row"><dt>Instance</dt><dd>{{ row.instance }}</dd></div>
        <div v-if="row?.owner"><dt>Current owner</dt><dd>{{ row.owner }}</dd></div>
        <div v-if="selection.kind === 'deleteUser'"><dt>Account</dt><dd>{{ selection.account || 'Standalone' }}</dd></div>
      </dl>
      <fieldset v-if="!result?.success" class="ceph-options panel-form" :disabled="busy">
        <label v-if="selection.kind === 'delete' || selection.kind === 'deleteUser'" class="check-label">
          <input v-model="purge" type="checkbox" @change="bypass = false; confirmation = ''" />
          {{ selection.kind === 'delete' ? 'Purge all bucket objects' : 'Purge user data' }}
        </label>
        <label v-if="selection.kind === 'delete' && purge" class="check-label">
          <input v-model="bypass" type="checkbox" @change="confirmation = ''" /> Bypass garbage collection
        </label>
        <template v-if="selection.kind === 'index'">
          <label class="check-label"><input v-model="repair" type="checkbox" @change="checkObjects = false; confirmation = ''" /> Repair index</label>
          <label v-if="repair" class="check-label"><input v-model="checkObjects" type="checkbox" @change="confirmation = ''" /> Check and reconcile object state</label>
        </template>
        <template v-if="selection.kind === 'link'">
          <label>Owner type<AppSelect v-model="ownerKind" :options="ownerKinds" :disabled="busy" @update:model-value="ownerId = ''; clearDestination()" /></label>
          <RgwIdentityPicker v-if="identityAvailable(profile, 'adminOps')" :key="ownerKind" :connection="profile.id"
            :kind="ownerKind" label="Find an owner" :disabled="busy" @select="lookup" />
          <details :open="!identityAvailable(profile, 'adminOps')">
            <summary>Enter an exact owner ID</summary>
            <div class="form-grid owner-fields">
              <label>{{ ownerKind === 'account' ? 'Account ID' : 'User ID' }}<input v-model.trim="ownerId" @input="clearDestination" @keydown.enter.prevent /></label>
              <label v-if="ownerKind === 'user'">User tenant<input v-model.trim="ownerTenant" @input="clearDestination" @keydown.enter.prevent /></label>
              <button type="button" :disabled="!ownerId" @click="lookup(ownerLookup(ownerKind, ownerId, ownerTenant))">Verify owner</button>
            </div>
          </details>
          <p v-if="destination">New owner {{ rgwTargetLabel(destination) }} · {{ destination.kind === 'account' ? destination.tenant || 'Default tenant' : destination.account || 'Standalone user' }}</p>
        </template>
      </fieldset>
      <Notice>{{ impact }}</Notice>
      <label v-if="phrase && !result?.success">Type <code>{{ phrase }}</code>
        <input v-model="confirmation" spellcheck="false" autocomplete="off" :disabled="busy" />
      </label>
      <Notice v-if="failure" tone="error">{{ failure }}</Notice>
      <CephActionResult v-if="result" :result="result" />
      <div class="modal-actions">
        <button type="button" :disabled="busy" @click="close">{{ result?.success ? 'Done' : 'Close' }}</button>
        <button v-if="!result?.success" :class="{ primary: !phrase, danger: !!phrase }" :disabled="busy || !action || phrase !== confirmation">
          {{ busy ? 'Waiting for RGW…' : selection.kind === 'index' && repair ? 'Repair index' : title }}
        </button>
      </div>
    </form>
  </Modal>
</template>

<style scoped>
.ceph-options { margin: 0; padding: 0; border: 0; min-width: 0; }
.owner-fields { align-items: end; margin-top: 8px; }
</style>
