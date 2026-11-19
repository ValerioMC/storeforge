<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { ArrowLeft, UserPlus } from 'lucide-vue-next'
import AppSelect from '../../../components/ui/AppSelect.vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import { portalApi } from '../api/portal'
import { sharingApi } from '../api/sharing'
import { usePortalTask } from '../composables/usePortalTask'
import { accessSources, projectRoleName, projectRoles, type Collaborator, type CollaboratorReview, type Collaborators, type MembershipReceipt } from '../model/sharing'
import type { PortalScope, PortalState, ProjectRole } from '../model/spaces'

const props = defineProps<{ scope: PortalScope; person?: number; manager: boolean }>()
const emit = defineEmits<{ person: [id?: number]; space: [id: string]; requests: [] }>()
const collaborators = usePortalTask<Collaborators>()
const review = usePortalTask<CollaboratorReview>()
const policy = usePortalTask<PortalState>()
const mutation = usePortalTask<MembershipReceipt | boolean>()
const { data: listing } = collaborators
const { data: detail } = review
const { data: state } = policy
const { data: result, pending, error } = mutation
const search = ref('')
const name = ref('')
const email = ref('')
const reason = ref('')
const role = ref<ProjectRole>('portal_user')
const action = ref<'add' | 'role' | 'remove' | 'revoke'>()
const spaceId = ref('')
const visible = computed(() => {
  const query = search.value.trim().toLowerCase()
  return listing.value?.collaborators.filter(person => !query || person.email.toLowerCase().includes(query) || person.display_name?.toLowerCase().includes(query)) ?? []
})

onMounted(() => {
  void refresh()
  void policy.run(() => portalApi.state(props.scope))
})
watch(() => props.person, id => {
  review.clear()
  if (id) void review.run(() => sharingApi.collaborator(props.scope, id))
}, { immediate: true })

async function refresh(): Promise<void> {
  await collaborators.run(() => sharingApi.collaborators(props.scope))
}

function openAction(next: typeof action.value, person?: Collaborator, space = ''): void {
  mutation.clear()
  action.value = next
  name.value = person?.display_name ?? ''
  email.value = person?.email ?? ''
  reason.value = ''
  role.value = person?.portal_role === 'portal_user' ? 'portal_manager' : 'portal_user'
  spaceId.value = space
}

async function submit(): Promise<void> {
  const requested = action.value
  const person = detail.value?.collaborator
  const message = reason.value.trim() || null
  const scope = props.scope
  if (!requested || pending.value) return
  const result = await mutation.run(async () => {
    if (requested === 'add') return sharingApi.addMember(scope, name.value.trim(), email.value.trim(), role.value, message)
    if (!person) return false
    if (requested === 'role') return sharingApi.memberRole(scope, person.user_id, role.value, message)
    if (requested === 'remove') return sharingApi.removeMember(scope, person.display_name, person.email, message)
    await sharingApi.revoke({ ...scope, space: spaceId.value }, person.user_id)
    return true
  })
  if (!result) return
  action.value = undefined
  await refresh()
  if (props.person) await review.run(() => sharingApi.collaborator(scope, props.person!))
}
</script>

<template>
  <Notice v-if="result && result !== true" :tone="result.status === 'failed' ? 'error' : 'info'">
    <div class="portal-inline">Request #{{ result.id }} · {{ result.status }} <span v-if="result.error_message">{{ result.error_message }}</span>
      <button type="button" @click="emit('requests')">Open requests</button>
    </div>
  </Notice>
  <Notice v-else-if="result === true" tone="success">Direct space access revoked.</Notice>
  <section v-if="!person" class="portal-panel">
    <div class="portal-between">
      <h2>Project membership</h2>
      <div class="portal-inline">
        <button type="button" @click="emit('requests')">Requests</button>
        <button v-if="manager" type="button" class="primary" @click="openAction('add')"><UserPlus :size="15" /> Add member</button>
      </div>
    </div>
    <Notice v-if="collaborators.error.value" tone="error">{{ collaborators.error.value }}</Notice>
    <p v-if="collaborators.pending.value && !listing" class="muted portal-note" role="status">Loading project membership…</p>
    <template v-if="listing">
      <label class="portal-inline">Find a collaborator <input v-model="search" type="search" /></label>
      <div class="portal-table-scroll">
        <table class="portal-table">
          <thead><tr><th scope="col">Name</th><th scope="col">Project role</th><th scope="col">Access source</th></tr></thead>
          <tbody>
            <tr v-for="member in visible" :key="member.user_id">
              <th scope="row">
                <button v-if="member.can_review_access" type="button" class="subtle portal-button-label" @click="emit('person', member.user_id)">{{ member.display_name || member.email }}</button>
                <span v-else>{{ member.display_name || member.email }}</span>
                <small v-if="member.display_name" class="muted portal-email">{{ member.email }}</small>
              </th>
              <td>{{ projectRoleName(member.portal_role) }}</td><td>{{ accessSources[member.access_source] }}</td>
            </tr>
            <tr v-if="!visible.length"><td colspan="3" class="muted">No matching collaborators.</td></tr>
          </tbody>
        </table>
      </div>
    </template>
  </section>
  <section v-else class="portal-panel">
    <div class="portal-between">
      <button type="button" class="subtle" @click="emit('person')"><ArrowLeft :size="15" /> All collaborators</button>
      <div v-if="detail" class="portal-inline">
        <button v-if="detail.can_request_role_change" type="button" @click="openAction('role', detail.collaborator)">Change project role</button>
        <button v-if="detail.can_request_project_removal" type="button" @click="openAction('remove', detail.collaborator)">Request project removal</button>
      </div>
    </div>
    <Notice v-if="review.error.value" tone="error">{{ review.error.value }}</Notice>
    <p v-if="review.pending.value &amp;&amp; !detail" class="muted portal-note" role="status">Loading access review…</p>
    <template v-if="detail">
      <div class="portal-between">
        <h2>{{ detail.collaborator.display_name || detail.collaborator.email }}</h2>
        <span class="chip">{{ projectRoleName(detail.collaborator.portal_role) }}</span>
      </div>
      <p class="muted portal-note">{{ detail.collaborator.email }} · {{ accessSources[detail.collaborator.access_source] }}</p>
      <ul v-if="detail.space_accesses.length" class="portal-list">
        <li v-for="space in detail.space_accesses" :key="`${space.storage_space_id}:${space.source}`" class="portal-between">
          <button type="button" class="subtle portal-button-label" @click="emit('space', space.storage_space_id)">{{ space.storage_space_name }}</button>
          <div class="portal-inline">
            <span class="chip">{{ space.role }}</span><span>{{ accessSources[space.source] }}</span>
            <button v-if="space.can_revoke" type="button" @click="openAction('revoke', detail.collaborator, space.storage_space_id)">Revoke grant</button>
          </div>
        </li>
      </ul>
      <p v-else class="muted portal-note">No storage spaces appear in this access review.</p>
    </template>
  </section>
  <Modal v-if="action" :title="{ add: 'Add project member', role: 'Change project role', remove: 'Request project removal', revoke: 'Revoke direct space grant' }[action]"
    @close="!pending && (action = undefined)">
    <form class="modal-form" @submit.prevent="submit">
      <template v-if="action === 'add'">
        <label class="portal-control">Name <input v-model="name" required maxlength="120" /></label>
        <label class="portal-control">Email <input v-model="email" type="email" required /></label>
      </template>
      <p v-else class="portal-note">{{ email }}</p>
      <AppSelect v-if="action === 'add' || action === 'role'" v-model="role" :options="projectRoles" aria-label="Project role" />
      <label v-if="action !== 'revoke'" class="portal-control">Reason <textarea v-model="reason" maxlength="2000" /></label>
      <p v-if="action === 'add'" class="muted portal-note">{{ state?.portal_collaborator_addition_delegated ? 'The service can apply delegated additions directly.' : 'Project additions require approval.' }}</p>
      <p v-if="action === 'role'" class="muted portal-note">{{ state?.portal_collaborator_role_management_delegated ? 'The service can apply delegated role changes directly.' : 'Project role changes require approval.' }}</p>
      <Notice v-if="error" tone="error">{{ error }}</Notice>
      <footer class="modal-actions">
        <button type="button" :disabled="pending" @click="action = undefined">Cancel</button>
        <button type="submit" class="primary" :disabled="pending || action === 'role' && role === detail?.collaborator.portal_role">{{ pending ? 'Sending…' : 'Confirm' }}</button>
      </footer>
    </form>
  </Modal>
</template>

<style scoped>
.portal-email { display: block; font-weight: 400; }
</style>
