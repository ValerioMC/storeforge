<script setup lang="ts">
import { computed, onScopeDispose, ref } from 'vue'
import { errorMessage } from '../../../common/tauri'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import BrowserWorkspace from '../../browser/components/BrowserWorkspace.vue'
import type { WorkbenchRow } from '../../bucket/model/workbench'
import { cephAction } from '../api/ceph-operations'
import { browserTarget, currentBucketTarget } from '../model/ceph-operations'
import CephActionDialog from './CephActionDialog.vue'

const props = defineProps<{ row: WorkbenchRow }>()
const emit = defineEmits<{ changed: [] }>()
const action = ref<'delete' | 'link' | 'unlink' | 'index'>()
const browser = ref(false)
const opening = ref(false)
const error = ref('')
const target = computed(() => browserTarget(props.row, props.row.profile))
const available = computed(() => currentBucketTarget(props.row.target, props.row.profile))
let disposed = false
onScopeDispose(() => { disposed = true })

async function browse(): Promise<void> {
  if (!target.value || opening.value) return
  opening.value = true
  error.value = ''
  try {
    const result = await cephAction(props.row.profile, { kind: 'bucket', target: props.row.target, owner: props.row.owner,
      change: { kind: 'browse' }, confirmation: '' })
    if (!disposed) {
      browser.value = result.success
      if (!result.success) error.value = result.message
    }
  } catch (failure) {
    if (!disposed) error.value = errorMessage(failure)
  } finally {
    opening.value = false
  }
}

function closeAction(changed: boolean): void {
  action.value = undefined
  if (changed) emit('changed')
}
</script>

<template>
  <div v-if="available" class="panel-actions" aria-label="RGW bucket actions">
    <button type="button" :disabled="!target || opening" :title="target ? undefined : 'Requires Browser access in this tenant'" @click="browse">
      {{ opening ? 'Verifying bucket…' : 'Browse objects' }}
    </button>
    <button type="button" @click="action = 'index'">Check index</button>
    <button type="button" @click="action = 'link'">Link owner</button>
    <button type="button" :disabled="!row.owner" @click="action = 'unlink'">Unlink owner</button>
    <button type="button" class="danger-outline" @click="action = 'delete'">Delete bucket</button>
  </div>
  <Notice v-if="error" tone="error">{{ error }}</Notice>
  <CephActionDialog v-if="action" :profile="row.profile" :selection="{ kind: action, row }" @close="closeAction" />
  <Modal v-if="browser && target" :title="`Browse ${row.name}`" drawer @close="browser = false">
    <div class="modal-form">
      <p class="muted">{{ row.profile.name }} · {{ row.tenant || 'Default tenant' }} · S3 identity</p>
      <BrowserWorkspace :embedded="target" :demo="false" compact />
    </div>
  </Modal>
</template>
