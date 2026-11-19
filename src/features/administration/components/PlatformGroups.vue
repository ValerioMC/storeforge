<script setup lang="ts">
import { ExternalLink, Pencil } from 'lucide-vue-next'
import type { PlatformGroup } from '../model/platform-governance'

defineProps<{ groups: PlatformGroup[]; busy: boolean }>()
const emit = defineEmits<{ edit: [group: PlatformGroup]; grants: [group: PlatformGroup]; review: [group: PlatformGroup] }>()

function associations(group: PlatformGroup): { label: string; names: string[] }[] {
  return [
    { label: 'Members', names: group.userDetails.map(user => user.fullName || user.email) },
    { label: 'RGW accounts', names: group.accountDetails.map(resource => resource.name) },
    { label: 'RGW users', names: group.s3UserDetails.map(resource => resource.name) },
    { label: 'Shared connections', names: group.s3ConnectionDetails.map(resource => resource.name) },
    { label: 'OpenStack clouds', names: group.openstackCloudDetails.map(resource => resource.name) },
  ].filter(association => association.names.length)
}
</script>

<template>
  <div class="settings-sheet">
    <div v-for="group in groups" :key="group.id" class="settings-row group-record">
      <div class="settings-description">
        <strong>{{ group.name }}</strong>
        <span v-if="group.systemKey === 'all_users'" class="chip">Implicit membership</span>
        <p v-if="group.description">{{ group.description }}</p>
        <details v-if="associations(group).length">
          <summary>Members and linked resources</summary>
          <dl><template v-for="association in associations(group)" :key="association.label">
            <dt>{{ association.label }}</dt><dd>{{ association.names.join(', ') }}</dd>
          </template></dl>
        </details>
      </div>
      <button type="button" :disabled="busy" @click="emit('review', group)">Review access</button>
      <button v-if="!group.systemKey" :aria-label="`Edit ${group.name}`" class="icon-button" type="button" :disabled="busy" @click="emit('edit', group)"><Pencil :size="15" /></button>
      <button type="button" :disabled="busy" @click="emit('grants', group)"><ExternalLink :size="15" /> Manage grants</button>
    </div>
  </div>
</template>

<style scoped>
.group-record { flex-wrap: wrap; }
.group-record .settings-description { min-width: min(250px, 100%); }
.group-record strong { margin-inline-end: 6px; }
details { margin-top: 5px; font-size: 12px; color: var(--muted); }
summary { cursor: pointer; }
dl { margin: 5px 0 0; }
dt { font-weight: 650; }
dd { margin: 0 0 4px; overflow-wrap: anywhere; }
</style>
