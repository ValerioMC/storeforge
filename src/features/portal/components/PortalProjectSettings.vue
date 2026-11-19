<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import { governanceApi } from '../api/governance'
import { usePortalTask } from '../composables/usePortalTask'
import { projectSettingNames, settingValue, type ProjectSettings, type SettingChange, type SettingKey, type SettingValue } from '../model/governance'
import type { MembershipReceipt } from '../model/sharing'
import type { PortalScope } from '../model/spaces'

const props = defineProps<{ scope: PortalScope; manager: boolean }>()
const emit = defineEmits<{ requests: [] }>()
const settings = usePortalTask<ProjectSettings>()
const change = usePortalTask<boolean>()
const { data: data } = settings
const { pending, error } = change
const request = ref<MembershipReceipt>()
const saved = ref(false)
const key = ref<SettingKey>()
const mode = ref<'inherit' | 'override'>('inherit')
const enabled = ref(true)
const days = ref(90)
const origins = ref('')
const reason = ref('')
const modes = [{ value: 'inherit', label: 'Inherit policy' }, { value: 'override', label: 'Set project override' }] as const
const editable = computed(() => data.value?.can_update || props.manager && !data.value?.delegated_to_portal_managers)

onMounted(() => void settings.run(() => governanceApi.settings(props.scope)))

function format(value: SettingValue | undefined): string {
  if (value === null || value === undefined) return 'Inherit'
  if (typeof value === 'boolean') return value ? 'Enabled' : 'Disabled'
  return Array.isArray(value) ? value.join(', ') || 'None' : `${value} days`
}

function select(field: SettingKey): void {
  if (!data.value) return
  change.clear()
  saved.value = false
  request.value = undefined
  reason.value = ''
  key.value = field
  const override = settingValue(data.value.project_override, field)
  const value = override ?? settingValue(data.value.effective, field)
  mode.value = override == null || platformLocked(field) ? 'inherit' : 'override'
  enabled.value = value === true
  days.value = typeof value === 'number' ? value : 90
  origins.value = Array.isArray(value) ? value.join('\n') : ''
}

function platformLocked(field: SettingKey): boolean {
  return data.value?.feature_states?.[field as keyof ProjectSettings['feature_states']]?.blocked_reason === 'platform_disabled'
}

function draft(field: SettingKey): SettingChange {
  const reset = mode.value === 'inherit'
  if (field === 'bucket_defaults.noncurrent_version_expiration_days') return { key: field, value: reset ? null : days.value }
  if (field === 'bucket_defaults.cors_allowed_origins') return { key: field, value: reset ? null : origins.value.split('\n').map(value => value.trim()).filter(Boolean) }
  return { key: field, value: reset ? null : enabled.value }
}

async function submit(): Promise<void> {
  if (!key.value || !editable.value || pending.value) return
  const setting = draft(key.value)
  const finished = await change.run(async () => {
    if (data.value?.can_update) {
      data.value = await governanceApi.saveSetting(props.scope, setting)
      saved.value = true
    } else {
      request.value = await governanceApi.requestSetting(props.scope, setting, reason.value.trim() || null)
    }
    return true
  })
  if (finished) key.value = undefined
}
</script>

<template>
  <section class="portal-panel">
    <h2>Effective project settings</h2>
    <Notice v-if="settings.error.value" tone="error">{{ settings.error.value }}</Notice>
    <Notice v-if="saved" tone="success">Project setting saved.</Notice>
    <Notice v-if="request" :tone="request.status === 'failed' ? 'error' : 'info'">
      <div class="portal-inline">Request #{{ request.id }} · {{ request.status }} <button type="button" @click="emit('requests')">Open requests</button></div>
    </Notice>
    <p v-if="settings.pending.value" role="status" class="muted portal-note">Loading project settings…</p>
    <template v-if="data">
      <p class="muted portal-note">{{ data.can_update ? 'Project setting changes are delegated to Portal Managers.' : manager ? 'Project overrides require approval.' : 'This project has read-only settings for your role.' }}</p>
      <div class="portal-table-scroll">
        <table class="portal-table">
          <thead><tr><th scope="col">Setting</th><th scope="col">Effective</th><th scope="col">Project override</th><th scope="col">Action</th></tr></thead>
          <tbody>
            <tr v-for="(label, field) in projectSettingNames" :key="field">
              <th scope="row">{{ label }}</th>
              <td>{{ format(settingValue(data.effective, field)) }}</td>
              <td>{{ format(settingValue(data.project_override, field)) }}</td>
              <td>
                <span v-if="platformLocked(field)" class="muted">Platform locked</span>
                <button v-if="editable && (data.can_update || field !== 'cost_estimation_enabled') && (!platformLocked(field) || settingValue(data.project_override, field) != null)"
                  type="button" :aria-label="`Edit ${label}`" @click="select(field)">
                  {{ platformLocked(field) ? 'Reset override' : data.can_update ? 'Edit' : 'Request change' }}
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <dl class="portal-statistics">
        <div><dt>Provider log retention</dt><dd>{{ data.effective.server_access_log_retention_days ?? '—' }}</dd></div>
        <div><dt>Maximum personal keys</dt><dd>{{ data.effective.max_portal_user_access_keys ?? '—' }}</dd></div>
      </dl>
    </template>
  </section>
  <Modal v-if="key" :title="projectSettingNames[key]" @close="!pending && (key = undefined)">
    <form class="modal-form" @submit.prevent="submit">
      <AppSelect v-model="mode" :options="modes" aria-label="Project setting mode" :disabled="platformLocked(key) || pending" />
      <template v-if="mode === 'override'">
        <label v-if="key === 'bucket_defaults.noncurrent_version_expiration_days'" class="portal-control">Days <input v-model.number="days" type="number" min="1" required /></label>
        <label v-else-if="key === 'bucket_defaults.cors_allowed_origins'" class="portal-control">One origin per line <textarea v-model="origins" /></label>
        <label v-else class="portal-inline"><input v-model="enabled" type="checkbox" /> Enabled</label>
      </template>
      <label v-if="!data?.can_update" class="portal-control">Reason <textarea v-model="reason" maxlength="2000" /></label>
      <Notice v-if="error" tone="error">{{ error }}</Notice>
      <footer class="modal-actions">
        <button type="button" :disabled="pending" @click="key = undefined">Cancel</button>
        <button type="submit" class="primary" :disabled="pending">{{ data?.can_update ? 'Save project setting' : 'Submit request' }}</button>
      </footer>
    </form>
  </Modal>
</template>
