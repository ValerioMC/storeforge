<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { Copy, Link2, Users } from 'lucide-vue-next'
import { formatDate } from '../../../common/format'
import AppSelect from '../../../components/ui/AppSelect.vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import { sharingApi } from '../api/sharing'
import { usePortalTask } from '../composables/usePortalTask'
import { grantRoles, type PublicLink, type SpaceAccessSummary, type SpaceShare } from '../model/sharing'
import type { Candidate, GrantRole, SpaceStatus, SpaceScope } from '../model/spaces'
import PortalCandidates from './PortalCandidates.vue'

const props = defineProps<{ scope: SpaceScope; status: SpaceStatus; objectKey?: string }>()
const emit = defineEmits<{ changed: [] }>()
const summary = usePortalTask<SpaceAccessSummary>()
const publicLinks = usePortalTask<PublicLink[]>()
const mutation = usePortalTask<boolean>()
const { data: access } = summary
const { data: links } = publicLinks
const { error, pending, run } = mutation
const action = ref<'audience' | 'grant' | 'role' | 'revoke' | 'createLink' | 'revokeLink'>()
const candidate = ref<Candidate>()
const share = ref<SpaceShare>()
const link = ref<PublicLink>()
const role = ref<GrantRole>('Viewer')
const audience = ref<'account' | 'restricted'>('restricted')
const object = ref('')
const label = ref('')
const expiry = ref('')
const message = ref('')
const active = computed(() => props.status !== 'Archived')
const modes = [{ value: 'account', label: 'Project team' }, { value: 'restricted', label: 'Selected people' }] as const

onMounted(async () => {
  await refresh()
  if (props.objectKey && access.value?.can_create_public_links && active.value) openAction('createLink')
})

async function refresh(): Promise<void> {
  const loaded = await summary.run(() => sharingApi.access(props.scope))
  if (loaded && (loaded.can_create_public_links || loaded.can_manage_access || loaded.public_link_count)) {
    await publicLinks.run(() => sharingApi.links(props.scope))
  }
}

function openAction(next: typeof action.value, target?: SpaceShare, publicLink?: PublicLink): void {
  mutation.clear()
  action.value = next
  candidate.value = undefined
  share.value = target
  link.value = publicLink
  role.value = target?.role ?? access.value?.default_account_member_role ?? 'Viewer'
  audience.value = access.value?.mode === 'all' ? 'account' : 'restricted'
  object.value = props.objectKey ?? ''
  label.value = ''
  expiry.value = ''
}

async function submit(): Promise<void> {
  const requested = action.value
  if (!requested || pending.value) return
  message.value = ''
  const done = await run(async () => {
    if (requested === 'audience') await sharingApi.setAudience(props.scope, audience.value, audience.value === 'account' ? role.value : null)
    if (requested === 'grant' && candidate.value) await sharingApi.grant(props.scope, candidate.value.user_id, role.value)
    if (requested === 'role' && share.value) await sharingApi.role(props.scope, share.value.user_id, role.value)
    if (requested === 'revoke' && share.value) await sharingApi.revoke(props.scope, share.value.user_id)
    if (requested === 'createLink') await sharingApi.createLink(props.scope, object.value, label.value.trim() || null,
      expiry.value ? new Date(expiry.value).toISOString() : null)
    if (requested === 'revokeLink' && link.value) await sharingApi.revokeLink(props.scope, link.value.id)
    return true
  })
  if (done) {
    action.value = undefined
    message.value = requested === 'createLink' ? 'Public link created.' : requested === 'revokeLink' ? 'Public link revoked.' : 'Space access updated.'
    await refresh()
    emit('changed')
  }
}

async function copy(link: PublicLink): Promise<void> {
  await run(async () => {
    await navigator.clipboard.writeText(link.url)
    message.value = 'Public link copied.'
    return true
  })
}
</script>

<template>
  <Notice v-if="summary.error.value" tone="error">{{ summary.error.value }}</Notice>
  <p v-if="summary.pending.value && !access" class="muted portal-note" role="status">Loading space access…</p>
  <Notice v-if="error && !action" tone="error">{{ error }}</Notice>
  <Notice v-if="message" tone="success">{{ message }}</Notice>
  <template v-if="access">
    <section class="portal-panel">
      <div class="portal-between">
        <h2><Users :size="16" /> People with access</h2>
        <div v-if="access.can_manage_access && active" class="portal-inline">
          <button v-if="access.mode !== 'private'" type="button" @click="openAction('audience')">Change team access</button>
          <button v-if="access.mode !== 'private'" type="button" class="primary" @click="openAction('grant')">Grant access</button>
        </div>
      </div>
      <p class="portal-note">
        <strong>{{ access.mode === 'all' ? 'Project team' : access.mode === 'private' ? 'Private' : 'Selected people' }}</strong>
        <span v-if="access.default_account_member_role"> · {{ access.default_account_member_role }} by default</span>
        <span> · {{ access.effective_member_count }} people</span>
      </p>
      <ul class="portal-list">
        <li v-if="access.owner" class="portal-between"><span>{{ access.owner.display_name || access.owner.email }}</span><span class="chip">Owner</span></li>
        <li v-for="grant in access.explicit_shares" :key="grant.user_id" class="portal-between">
          <span>{{ grant.email }}</span>
          <div class="portal-inline">
            <span class="chip">{{ grant.role }}</span><span class="muted">Direct grant</span>
            <template v-if="access.can_manage_access && active">
              <button type="button" @click="openAction('role', grant)">Change role</button>
              <button type="button" @click="openAction('revoke', grant)">Revoke</button>
            </template>
          </div>
        </li>
      </ul>
    </section>
    <section v-if="access.can_create_public_links || access.can_manage_access || access.public_link_count" class="portal-panel">
      <div class="portal-between">
        <h2><Link2 :size="16" /> Public links</h2>
        <button v-if="access.can_create_public_links && active" type="button" @click="openAction('createLink')">Create public link</button>
      </div>
      <Notice v-if="publicLinks.error.value" tone="error">{{ publicLinks.error.value }}</Notice>
      <ul v-if="links?.length" class="portal-list">
        <li v-for="item in links" :key="item.id" class="portal-between">
          <div><strong>{{ item.object_name }}</strong><div class="muted">{{ item.status }}<span v-if="item.expires_at"> · Expires {{ formatDate(item.expires_at, true) }}</span></div></div>
          <div v-if="item.status === 'Active'" class="portal-inline">
            <button type="button" :disabled="pending" @click="copy(item)"><Copy :size="14" /> Copy link</button>
            <button v-if="access.can_create_public_links || access.can_manage_access" type="button" @click="openAction('revokeLink', undefined, item)">Revoke link</button>
          </div>
        </li>
      </ul>
      <p v-else-if="links" class="muted portal-note">No public links for this space.</p>
    </section>
  </template>
  <Modal v-if="action" :title="{ audience: 'Project team access', grant: 'Grant space access', role: 'Change space role', revoke: 'Revoke direct access', createLink: 'Create a public link', revokeLink: 'Revoke public link' }[action]"
    @close="!pending && (action = undefined)">
    <form class="modal-form" @submit.prevent="submit">
      <AppSelect v-if="action === 'audience'" v-model="audience" :options="modes" aria-label="Space audience" />
      <template v-if="action === 'grant'">
        <p v-if="candidate" class="portal-note">Selected {{ candidate.display_name || candidate.email }}</p>
        <PortalCandidates :scope="scope" :selected="candidate ? [candidate.user_id] : []" @pick="candidate = $event" />
      </template>
      <AppSelect v-if="action === 'grant' || action === 'role' || action === 'audience' && audience === 'account'" v-model="role" :options="grantRoles" aria-label="Space role" />
      <template v-if="action === 'createLink'">
        <p class="portal-note">Anyone with the link can download the file while the link is active.</p>
        <label class="portal-control">File key <input v-model="object" required /></label>
        <label class="portal-control">Label <input v-model="label" maxlength="120" /></label>
        <label class="portal-control">Expires at <input v-model="expiry" type="datetime-local" /></label>
      </template>
      <p v-if="action === 'revoke'">Remove the direct grant for {{ share?.email }}? Project or group access may still apply.</p>
      <p v-if="action === 'revokeLink'">Stop new downloads using the link for {{ link?.object_name }}.</p>
      <Notice v-if="error" tone="error">{{ error }}</Notice>
      <footer class="modal-actions">
        <button type="button" :disabled="pending" @click="action = undefined">Cancel</button>
        <button type="submit" class="primary" :disabled="pending || action === 'grant' && !candidate">{{ pending ? 'Saving…' : 'Confirm' }}</button>
      </footer>
    </form>
  </Modal>
</template>

<style scoped>
h2 svg { vertical-align: -3px; margin-right: 3px; }
</style>
