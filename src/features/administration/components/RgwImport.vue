<script setup lang="ts">
import { computed, ref } from 'vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import { listRgwIdentities } from '../api/rgw'
import { useQueryPage } from '../composables/useQueryPage'
import { parseRgwUserLabel, rgwTargetKey, rgwTargetLabel, type RgwChange, type RgwKind, type RgwTarget } from '../model/rgw'
import QueryPager from './QueryPager.vue'

const props = defineProps<{ connection: string; kind: RgwKind; busy: boolean; error: string }>()
const emit = defineEmits<{ close: []; save: [change: RgwChange] }>()
const selected = ref<RgwTarget[]>([])
const roots = ref<Record<string, string>>({})
const { items, markers, nextMarker, loading, error: listError, search, next, previous, reload } = useQueryPage(
  () => props.kind, marker => listRgwIdentities(props.connection, props.kind, false, marker), row => rgwTargetLabel(row.target),
  10,
)
const selectedKeys = computed(() => new Set(selected.value.map(rgwTargetKey)))
const limit = 50

function toggle(target: RgwTarget): void {
  const key = rgwTargetKey(target)
  if (selectedKeys.value.has(key)) selected.value = selected.value.filter(item => rgwTargetKey(item) !== key)
  else if (selected.value.length < limit) selected.value = [...selected.value, target]
}

function submit(): void {
  if (!selected.value.length || props.busy) return
  if (props.kind === 'account') {
    const entries = selected.value.flatMap(target => {
      if (target.kind !== 'account') return []
      const root = roots.value[target.id]?.trim()
      return [{ id: target.id, root: root ? parseRgwUserLabel(root) : null }]
    })
    emit('save', { action: 'importAccounts', entries })
  } else {
    emit('save', { action: 'importUsers', identities: selected.value.flatMap(target => target.kind === 'user' ? [target.identity] : []) })
  }
}
</script>

<template>
  <Modal :title="`Import RGW ${kind}s`" @close="!busy && emit('close')">
    <div class="modal-form">
      <p>Register up to {{ limit }} existing identities from this endpoint.</p>
      <div class="form-grid">
        <label>Find {{ kind }}<input v-model="search" type="search" @keydown.enter.prevent="reload" /></label>
        <button type="button" :disabled="loading || busy" @click="reload">Search</button>
      </div>
      <Notice v-if="error || listError" tone="error">{{ error || listError }}</Notice>
      <p v-if="loading" role="status">Reading cluster inventory…</p>
      <p v-else-if="!items.length" class="muted">No matching cluster identities.</p>
      <div v-else class="rgw-import-options">
        <label v-for="{ target, managed } in items" :key="rgwTargetKey(target)" class="settings-row">
          <input type="checkbox" :checked="selectedKeys.has(rgwTargetKey(target))" :disabled="busy || managed || (selected.length >= limit && !selectedKeys.has(rgwTargetKey(target)))" @change="toggle(target)" />
          <span>{{ rgwTargetLabel(target) }}<small v-if="managed"> · Managed</small></span>
        </label>
      </div>
      <QueryPager :busy="loading || busy" :previous="!!markers.length" :next="!!nextMarker" @previous="previous" @next="next" />
      <div v-if="selected.length" class="panel-form">
        <div v-for="target in selected" :key="rgwTargetKey(target)" class="settings-row">
          <div class="settings-description"><strong>{{ rgwTargetLabel(target) }}</strong></div>
          <label v-if="target.kind === 'account'">Root UID for {{ target.id }}
            <input v-model="roots[target.id]" placeholder="Discover automatically" :disabled="busy" spellcheck="false" />
          </label>
          <button type="button" :disabled="busy" :aria-label="`Remove ${rgwTargetLabel(target)}`" @click="toggle(target)">Remove</button>
        </div>
      </div>
      <div class="modal-actions">
        <button type="button" :disabled="busy" @click="emit('close')">Cancel</button>
        <button class="primary" type="button" :disabled="busy || !selected.length" @click="submit">{{ busy ? 'Verifying identities…' : `Import ${selected.length} selected` }}</button>
      </div>
    </div>
  </Modal>
</template>

<style scoped>
.form-grid { align-items: end; }
.rgw-import-options { max-height: 220px; overflow: auto; border: 1px solid var(--line); border-radius: 7px; }
.rgw-import-options .settings-row { justify-content: start; }
</style>
