<script setup lang="ts">
import { ref } from 'vue'
import { Cable, CirclePlus, Pencil, Server, Trash2 } from 'lucide-vue-next'
import EmptyState from '../../../components/ui/EmptyState.vue'
import ConfirmDialog from '../../../components/ui/ConfirmDialog.vue'
import Notice from '../../../components/ui/Notice.vue'
import { errorMessage } from '../../../common/tauri'
import { capabilityNames, identityCapability, providerNames, type Connection } from '../model/connection'
import { useConnections } from '../store/connections'

const emit = defineEmits<{ add: []; edit: [profile: Connection]; browse: [] }>()
const connections = useConnections()
const removing = ref<Connection>()
const error = ref('')

function useProfile(profile: Connection): void {
  connections.select(profile.id)
  emit('browse')
}

async function removeProfile(): Promise<void> {
  const profile = removing.value
  if (!profile) return
  removing.value = undefined
  try {
    await connections.remove(profile.id)
    error.value = ''
  } catch (failure) {
    error.value = errorMessage(failure)
  }
}
</script>

<template>
  <section class="workspace-page">
    <div class="page-header">
      <div><h1>Connections</h1><p>Manage endpoints and choose the credentials for your workspace.</p></div>
      <button class="primary" @click="emit('add')"><CirclePlus :size="17" /> Add connection</button>
    </div>
    <Notice v-if="error" tone="error">{{ error }}</Notice>
    <div v-if="connections.profiles.length" class="settings-sheet">
      <div v-for="profile in connections.profiles" :key="profile.id" class="settings-row">
        <span class="file-icon"><Server :size="19" /></span>
        <div class="settings-description">
          <strong>{{ profile.name }}</strong>
          <p>{{ providerNames[profile.provider] }} · {{ profile.region }} · {{ profile.endpoint || 'AWS regional endpoint' }}</p>
        </div>
        <span v-if="profile.settings?.defaultEndpoint" class="chip">Default</span>
        <span v-for="tag in profile.settings?.standardTags" :key="'standard-' + tag" class="chip">{{ tag }}</span>
        <span v-for="tag in profile.settings?.administrativeTags" :key="'admin-' + tag" class="chip" :title="'Administrative tag: ' + tag">{{ tag }}</span>
        <span v-if="profile.id === connections.activeId" class="chip chip-active">Active</span>
        <button :disabled="['denied', 'unavailable'].includes(identityCapability(profile, 's3'))"
          :title="capabilityNames[identityCapability(profile, 's3')]" @click="useProfile(profile)">Browse buckets</button>
        <button class="icon-button subtle" :aria-label="`Edit ${profile.name}`" @click="emit('edit', profile)"><Pencil :size="16" /></button>
        <button class="icon-button subtle" :aria-label="`Remove ${profile.name}`" @click="removing = profile"><Trash2 :size="16" /></button>
      </div>
    </div>
    <EmptyState v-else :icon="Cable" title="No saved connections" description="Add an Amazon S3, Ceph RGW or other S3-compatible endpoint.">
      <button class="primary" @click="emit('add')">Add connection</button>
    </EmptyState>
    <ConfirmDialog v-if="removing" title="Remove connection?" :description="`${removing.name} and its credentials will be removed from this device.`"
      action="Remove connection" danger @close="removing = undefined" @confirm="removeProfile" />
  </section>
</template>
