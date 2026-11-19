<script setup lang="ts">
import { computed, onScopeDispose, ref } from 'vue'
import { errorMessage } from '../../../common/tauri'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import type { BucketTarget } from '../../bucket/model/workbench'
import type { Connection } from '../../connection/model/connection'
import { checkCephIndexes } from '../api/ceph-operations'
import { cephBucketLabel, type IndexOutcome } from '../model/ceph-operations'
import CephActionResult from './CephActionResult.vue'

const props = defineProps<{ profile: Connection; targets: BucketTarget[] }>()
const emit = defineEmits<{ close: [] }>()
const records = ref<IndexOutcome[]>([])
const error = ref('')
const running = ref(false)
const finished = ref(false)
const failed = computed(() => records.value.filter(record => !record.result.success).length)
const endpoint = computed(() => props.profile.services?.cephAdmin?.endpoint || props.profile.endpoint)
let disposed = false
onScopeDispose(() => { disposed = true })

async function run(): Promise<void> {
  if (running.value) return
  records.value = []
  error.value = ''
  running.value = true
  try {
    await checkCephIndexes(props.profile.id, props.targets, record => {
      if (!disposed) records.value.push(record)
    })
    finished.value = true
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    running.value = false
  }
}

function close(): void {
  if (!running.value) emit('close')
}
</script>

<template>
  <Modal title="Check selected bucket indexes" wide @close="close">
    <div class="modal-form">
      <p class="muted">{{ profile.name }} · {{ endpoint }}</p>
      <Notice>Run read-only RGW diagnostics on {{ targets.length }} selected bucket instances.</Notice>
      <details v-if="!records.length">
        <summary>Selected buckets</summary>
        <ul><li v-for="target in targets" :key="cephBucketLabel(target)">{{ cephBucketLabel(target) }}</li></ul>
      </details>
      <p v-if="running || records.length" role="status">{{ records.length }} of {{ targets.length }} checked · {{ failed }} failed</p>
      <progress v-if="running" :value="records.length" :max="targets.length" aria-label="Bucket index checks completed" />
      <Notice v-if="error" tone="error">{{ error }}</Notice>
      <div class="batch-results">
        <details v-for="{ target, result } in records" :key="cephBucketLabel(target)">
          <summary>{{ cephBucketLabel(target) }} · {{ result.success ? 'Checked' : 'Failed' }}<template v-if="result.status !== null"> · HTTP {{ result.status }}</template></summary>
          <CephActionResult :result="result" />
        </details>
      </div>
      <div class="modal-actions">
        <button type="button" :disabled="running" @click="close">{{ finished ? 'Done' : 'Close' }}</button>
        <button v-if="!finished" type="button" class="primary" :disabled="running" @click="run">{{ running ? 'Checking…' : 'Run checks' }}</button>
      </div>
    </div>
  </Modal>
</template>

<style scoped>
.batch-results { display: grid; gap: 6px; max-height: 330px; overflow: auto; }
progress { width: 100%; }
</style>
