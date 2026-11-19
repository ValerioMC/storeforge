<script setup lang="ts">
import { computed, ref } from 'vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import { portalApi } from '../api/portal'
import { usePortalTask } from '../composables/usePortalTask'
import { provisionSpace, type Candidate, type GrantRole, type PortalScope, type PortalSpace, type PortalState, type SpaceAudience } from '../model/spaces'
import PortalCandidates from './PortalCandidates.vue'

const props = defineProps<{ scope: PortalScope; state: PortalState; importing: boolean }>()
const emit = defineEmits<{ close: []; created: [space: PortalSpace] }>()
const audiences = computed<{ value: SpaceAudience; label: string }[]>(() => [
  ...(props.state.can_create_private_storage_spaces ? [{ value: 'private' as const, label: 'Private' }] : []),
  ...(props.state.can_create_team_storage_spaces ? [{ value: 'team' as const, label: 'Project team' }, { value: 'selected' as const, label: 'Selected people' }] : []),
])
const roles = [{ value: 'Viewer', label: 'Viewer' }, { value: 'Editor', label: 'Editor' }] as const
const audience = ref<SpaceAudience>(audiences.value[0]?.value ?? 'private')
const name = ref('')
const description = ref('')
const teamRole = ref<GrantRole>('Viewer')
const named = ref(false)
const people = ref<{ user: Candidate; role: GrantRole }[]>([])
const { pending, error, run } = usePortalTask<PortalSpace>()

async function submit(): Promise<void> {
  const access = provisionSpace(audience.value, description.value, teamRole.value,
    people.value.map(({ user, role }) => ({ user_id: user.user_id, role })))
  const space = await run(() => props.importing
    ? portalApi.import(props.scope, { ...access, bucket_name: name.value.trim() })
    : portalApi.create(props.scope, { ...access, name: name.value.trim(), naming_mode: named.value ? 'named_bucket' : 'generic_uuid' }))
  if (space) emit('created', space)
}
</script>

<template>
  <Modal :title="importing ? 'Import a bucket' : 'Create storage space'" wide @close="!pending && emit('close')">
    <form class="modal-form" @submit.prevent="submit">
      <label class="portal-control">{{ importing ? 'Existing bucket name' : 'Space name' }}
        <input v-model="name" required :maxlength="importing || named ? 63 : 255" :disabled="pending" />
      </label>
      <label v-if="!importing && state.allow_named_bucket_create" class="portal-inline"><input v-model="named" type="checkbox" /> Use the space name as the bucket ID</label>
      <label class="portal-control">Description <textarea v-model="description" maxlength="2000" :disabled="pending" /></label>
      <AppSelect v-model="audience" aria-label="Who can access this space?" :options="audiences" :disabled="pending" />
      <AppSelect v-if="audience === 'team'" v-model="teamRole" aria-label="Project member role" :options="roles" :disabled="pending" />
      <template v-if="audience === 'selected'">
        <PortalCandidates :scope="scope" :selected="people.map(({ user }) => user.user_id)" @pick="people.push({ user: $event, role: 'Viewer' })" />
        <ul v-if="people.length" class="portal-list">
          <li v-for="(entry, index) in people" :key="entry.user.user_id" class="portal-between">
            <span>{{ entry.user.display_name || entry.user.email }}</span>
            <div class="portal-inline">
              <AppSelect v-model="entry.role" :aria-label="`Role for ${entry.user.email}`" :options="roles" />
              <button type="button" :aria-label="`Remove ${entry.user.email}`" @click="people.splice(index, 1)">Remove</button>
            </div>
          </li>
        </ul>
      </template>
      <Notice v-if="error" tone="error">{{ error }}</Notice>
      <footer class="modal-actions">
        <button type="button" :disabled="pending" @click="emit('close')">Cancel</button>
        <button type="submit" class="primary" :disabled="pending || !name.trim() || audience === 'selected' && !people.length">
          {{ pending ? 'Saving…' : importing ? 'Import bucket' : 'Create space' }}
        </button>
      </footer>
    </form>
  </Modal>
</template>
