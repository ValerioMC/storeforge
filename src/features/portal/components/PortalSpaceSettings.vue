<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import Notice from '../../../components/ui/Notice.vue'
import { portalApi } from '../api/portal'
import { usePortalTask } from '../composables/usePortalTask'
import type { SpaceScope, SpaceSettings, SpaceSettingsDraft } from '../model/spaces'

const props = defineProps<{ scope: SpaceScope; editable: boolean }>()
const { data, pending, error, run } = usePortalTask<SpaceSettings>()
const draft = ref<SpaceSettingsDraft>()
const message = ref('')
const dirty = computed(() => draft.value && data.value &&
  (draft.value.versioning_enabled !== data.value.versioning_enabled
    || draft.value.lifecycle_enabled !== data.value.lifecycle_enabled
    || draft.value.version_history_retention_days !== data.value.version_history_retention_days))

onMounted(async () => {
  const settings = await run(() => portalApi.settings(props.scope))
  if (settings) copyDraft(settings)
})

function copyDraft(settings: SpaceSettings): void {
  draft.value = {
    versioning_enabled: settings.versioning_enabled, lifecycle_enabled: settings.lifecycle_enabled,
    version_history_retention_days: settings.version_history_retention_days,
  }
}

async function save(): Promise<void> {
  if (!draft.value || !props.editable || !data.value?.can_update || pending.value) return
  message.value = ''
  const settings = await run(() => portalApi.saveSettings(props.scope, { ...draft.value! }))
  if (settings) {
    copyDraft(settings)
    message.value = 'Space settings saved.'
  }
}
</script>

<template>
  <section class="portal-panel">
    <h2>Versioning and retention</h2>
    <Notice v-if="error" tone="error">{{ error }}</Notice>
    <Notice v-if="message" tone="success">{{ message }}</Notice>
    <p v-if="pending && !data" class="muted portal-note" role="status">Loading space settings…</p>
    <form v-if="data && draft" class="space-settings" @submit.prevent="save">
      <p class="portal-note">Current versioning status <strong>{{ data.versioning_status }}</strong></p>
      <label class="portal-inline"><input v-model="draft.versioning_enabled" type="checkbox" :disabled="!editable || !data.can_update || pending" /> Keep file versions</label>
      <label class="portal-inline"><input v-model="draft.lifecycle_enabled" type="checkbox" :disabled="!editable || !data.can_update || pending" /> Expire historical versions</label>
      <label class="portal-control">History retention in days
        <input v-model.number="draft.version_history_retention_days" type="number" min="1" required :disabled="!editable || !data.can_update || pending || !draft.lifecycle_enabled" />
      </label>
      <p class="muted portal-note">Retention applies to historical versions. The service retains other lifecycle rules.</p>
      <div v-if="editable && data.can_update"><button type="submit" class="primary" :disabled="pending || !dirty">{{ pending ? 'Saving…' : 'Save settings' }}</button></div>
    </form>
  </section>
</template>

<style scoped>
.space-settings { display: grid; gap: 12px; align-items: start; }
.space-settings input[type=number] { max-width: 180px; }
</style>
