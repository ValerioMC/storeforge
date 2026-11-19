<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { errorMessage } from '../../../common/tauri'
import ConfirmDialog from '../../../components/ui/ConfirmDialog.vue'
import Notice from '../../../components/ui/Notice.vue'
import type { Connection } from '../../connection/model/connection'
import { bucketsApi } from '../api/buckets'
import type { BucketQuota } from '../model/bucket'

const props = defineProps<{ profile: Connection; bucket: string; active: boolean }>()
const emit = defineEmits<{ changed: [] }>()
const original = ref<BucketQuota>()
const draft = ref<BucketQuota>()
const saving = ref(false)
const loading = ref(false)
const readError = ref('')
const writeError = ref('')
const reviewing = ref(false)
const dirty = computed(() => JSON.stringify(draft.value) !== JSON.stringify(original.value))
let loaded = false

watch(() => props.active, active => {
  if (active && !loaded) void reload()
}, { immediate: true })

async function reload(): Promise<void> {
  if (loading.value) return
  loading.value = true
  loaded = true
  readError.value = ''
  try {
    const quota = await bucketsApi.quota(props.profile.id, props.bucket)
    if (!dirty.value) draft.value = { ...quota }
    original.value = quota
  } catch (failure) {
    readError.value = errorMessage(failure)
  } finally {
    loading.value = false
  }
}

async function apply(): Promise<void> {
  reviewing.value = false
  if (!draft.value || !original.value || saving.value) return
  saving.value = true
  writeError.value = ''
  try {
    await bucketsApi.updateQuota(props.profile.id, props.bucket, original.value, draft.value)
    original.value = { ...draft.value }
    await reload()
    emit('changed')
  } catch (failure) {
    await reload()
    writeError.value = errorMessage(failure)
  } finally {
    saving.value = false
  }
}

function discard(): void {
  draft.value = original.value && { ...original.value }
}

async function refresh(): Promise<void> {
  if (loaded && !saving.value) await reload()
}

defineExpose({ dirty, saving, discard, refresh })
</script>

<template>
  <div class="workspace-drawer-content bucket-quota">
    <div class="bucket-detail-heading">
      <div><h3>Bucket quota</h3><p class="muted">Limits enforced by the Ceph gateway.</p></div>
      <button :disabled="saving || loading" @click="reload">Reload quota</button>
    </div>
    <Notice v-if="readError" tone="error">Quota read failed. {{ readError }}</Notice>
    <Notice v-if="writeError" tone="error">{{ writeError }}</Notice>
    <p v-if="loading" role="status" class="muted">Reading quota…</p>
    <form v-if="draft" class="modal-form" @submit.prevent="reviewing = true">
      <label class="check-label"><input v-model="draft.enabled" type="checkbox" :disabled="saving || loading" /> Enforce bucket quota</label>
      <label>Storage limit in bytes<input v-model="draft.maxBytes" inputmode="numeric" pattern="[0-9]*" placeholder="Unlimited" :disabled="saving || loading" /></label>
      <label>Object limit<input v-model="draft.maxObjects" inputmode="numeric" pattern="[0-9]*" placeholder="Unlimited" :disabled="saving || loading" /></label>
      <div class="panel-actions">
        <button v-if="dirty" type="button" :disabled="saving || loading" @click="discard">Discard draft</button>
        <button class="primary" :disabled="!dirty || saving || loading || !!readError">Apply quota</button>
      </div>
    </form>
    <ConfirmDialog v-if="reviewing" title="Apply bucket quota?" :description="`These limits apply to ${bucket} on ${profile.name}.`"
      action="Apply quota" @close="reviewing = false" @confirm="apply" />
  </div>
</template>

<style scoped>
.bucket-quota { padding: 16px; }
.bucket-quota .modal-form { max-width: 420px; padding-inline: 0; }
</style>
