<script setup lang="ts">
import { computed } from 'vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import WorkspaceHeader from '../../../components/shell/WorkspaceHeader.vue'
import type { Connection } from '../model/connection'

const props = defineProps<{ profiles: Connection[] }>()
const compact = defineModel<boolean>('compact', { required: true })
const showTags = defineModel<boolean>('showTags', { required: true })
const startup = defineModel<string>('startup', { required: true })
const startupOptions = computed(() => [
  { value: '', label: 'Last workspace' },
  ...props.profiles.map(profile => ({ value: profile.id, label: profile.name })),
])
</script>

<template>
  <section class="workspace-page">
    <WorkspaceHeader title="Preferences" description="Workspace preferences for this device." />
    <div class="settings-sheet workspace-preferences">
      <div class="settings-row">
        <div class="settings-description"><strong>Startup context</strong><p>Open the last workspace or the default workspace of a saved profile.</p></div>
        <AppSelect v-model="startup" :options="startupOptions" aria-label="Startup context" />
      </div>
      <div class="settings-row"><label class="check-row"><input v-model="compact" type="checkbox" /> Compact rows</label></div>
      <div class="settings-row"><label class="check-row"><input v-model="showTags" type="checkbox" /> Show standard tags in the context selector</label></div>
    </div>
  </section>
</template>
