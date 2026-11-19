<script setup lang="ts">
import { computed, onScopeDispose, ref } from 'vue'
import { ArrowLeft, Plus } from 'lucide-vue-next'
import { errorMessage } from '../../../common/tauri'
import WorkspaceTabs from '../../../components/shell/WorkspaceTabs.vue'
import ConfirmDialog from '../../../components/ui/ConfirmDialog.vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import FadeRegion from '../../../components/ui/FadeRegion.vue'
import type { Provider } from '../../connection/model/connection'
import { writeQuery } from '../api/iam'
import { createRole, inspectRole, listRoles, updateRole, type RoleDraft } from '../api/resources'
import { useQueryPage } from '../composables/useQueryPage'
import { validPolicy } from '../model/iam'
import { validRoleDuration, type RoleDetails, type RoleTag } from '../model/resources'
import IamPolicies from './IamPolicies.vue'
import QueryPager from './QueryPager.vue'

const props = defineProps<{ connection: string; provider: Provider }>()
const tabs = [
  { value: 'general', label: 'General' }, { value: 'trust', label: 'Trust policy' }, { value: 'policies', label: 'Policies' },
] as const
const tab = ref<(typeof tabs)[number]['value']>('general')
const selected = ref('')
const role = ref<RoleDetails>()
const duration = ref(3600)
const trust = ref('')
const tags = ref<RoleTag[]>([])
const creating = ref<RoleDraft>()
const removing = ref(false)
const busy = ref(false)
const error = ref('')
const loadError = ref('')
const { items, markers, nextMarker, loading, error: listError, search, next, previous, reload } = useQueryPage(
  () => props.connection, marker => listRoles(props.connection, marker), row => `${row.name} ${row.path} ${row.arn}`,
)
const tagError = computed(() => {
  if (tags.value.some(tag => !tag.key.trim())) return 'Enter a key for each role tag.'
  if (props.provider === 'aws' && new Set(tags.value.map(tag => tag.key.toLowerCase())).size !== tags.value.length) {
    return 'AWS IAM supports one value per role tag key.'
  }
  if (tags.value.length > 50) return 'A role can have up to 50 tags.'
  return ''
})
let generation = 0
onScopeDispose(() => { generation++ })

async function refresh(name: string, current: number): Promise<void> {
  try {
    const result = await inspectRole(props.connection, name)
    if (generation !== current) return
    role.value = result
    duration.value = result.duration
    trust.value = result.trust
    tags.value = result.tags.map(tag => ({ ...tag }))
    loadError.value = ''
  } catch (failure) {
    if (generation === current) loadError.value = errorMessage(failure)
  }
}

async function open(name: string): Promise<void> {
  const current = ++generation
  selected.value = name
  role.value = undefined
  tab.value = 'general'
  error.value = ''
  loadError.value = ''
  busy.value = true
  await refresh(name, current)
  if (generation === current) busy.value = false
}

function close(): void {
  generation++
  selected.value = ''
  role.value = undefined
  busy.value = false
  error.value = ''
  loadError.value = ''
}

async function create(): Promise<void> {
  if (!creating.value || busy.value) return
  const current = ++generation
  busy.value = true
  error.value = ''
  try {
    const name = await createRole(props.connection, { ...creating.value, name: creating.value.name.trim() })
    if (current !== generation) return
    creating.value = undefined
    await open(name)
    await reload()
  } catch (failure) {
    if (current === generation) error.value = errorMessage(failure)
  } finally {
    if (current === generation) busy.value = false
  }
}

async function apply(action: 'general' | 'trust' | 'delete'): Promise<void> {
  const baseline = role.value
  if (!baseline || busy.value) return
  const current = ++generation
  busy.value = true
  removing.value = false
  error.value = ''
  try {
    if (action === 'general') await updateRole(props.connection, props.provider, baseline, { duration: duration.value, tags: tags.value })
    if (action === 'trust') await writeQuery(props.connection, 'UpdateAssumeRolePolicy', { RoleName: baseline.name, PolicyDocument: trust.value })
    if (action === 'delete') {
      await writeQuery(props.connection, 'DeleteRole', { RoleName: baseline.name })
      if (current === generation) {
        close()
        await reload()
      }
      return
    }
  } catch (failure) {
    if (generation === current) error.value = errorMessage(failure)
  }
  if (generation === current) {
    await refresh(baseline.name, current)
    busy.value = false
  }
}

function newRole(): void {
  error.value = ''
  creating.value = { name: '', path: '/', duration: 3600, trust: '' }
}
</script>

<template>
  <div class="panel-form">
    <template v-if="!selected">
      <div class="stage-heading"><h2>IAM roles</h2><button class="primary" @click="newRole"><Plus :size="16" /> Create role</button></div>
      <form class="form-grid" @submit.prevent="reload">
        <label>Search roles<input v-model="search" type="search" /></label>
        <div class="panel-actions"><button :disabled="loading">Search</button><button type="button" :disabled="loading" @click="reload">Refresh</button></div>
      </form>
      <Notice v-if="listError" tone="error">{{ listError }}</Notice>
      <p v-else-if="loading" role="status">Loading roles…</p>
      <p v-else-if="!items.length" class="muted">No matching roles.</p>
      <div v-for="item in items" :key="item.id" class="settings-row">
        <div class="settings-description"><strong>{{ item.name }}</strong><p>{{ item.arn }}</p></div>
        <button :aria-label="`Open ${item.name}`" @click="open(item.name)">Open</button>
      </div>
      <QueryPager :busy="loading" :previous="!!markers.length" :next="!!nextMarker" @previous="previous" @next="next" />
    </template>
    <template v-else>
      <button :disabled="busy" @click="close"><ArrowLeft :size="15" /> IAM roles</button>
      <div class="stage-heading">
        <div><h2>{{ selected }}</h2><p v-if="role">{{ role.arn }}</p></div>
        <button v-if="role" :disabled="busy" @click="removing = true">Delete role</button>
      </div>
      <Notice v-if="error" tone="error">{{ error }}</Notice>
      <Notice v-if="loadError" tone="error">{{ loadError }} <button :disabled="busy" @click="open(selected)">Retry</button></Notice>
      <p v-if="busy && !role" role="status">Loading role…</p>
      <template v-if="role">
        <WorkspaceTabs v-model="tab" :tabs="tabs" label="IAM role sections" />
        <FadeRegion>
          <div :key="tab">
            <form v-if="tab === 'general'" class="panel-form" @submit.prevent="apply('general')">
              <div class="form-grid">
                <label>IAM path<input :value="role.path" readonly /></label>
                <label>Maximum session duration in seconds<input v-model.number="duration" type="number" min="3600" max="43200" required :disabled="busy || !!loadError" /></label>
              </div>
              <section class="panel-form">
                <div class="stage-heading"><h3>Role tags</h3><button type="button" :disabled="busy || !!role.tagError || !!loadError" @click="tags.push({ key: '', value: '' })">Add tag</button></div>
                <Notice v-if="role.tagError">{{ role.tagError }}</Notice>
                <div v-for="(tag, index) in tags" :key="index" class="settings-row">
                  <label>Tag key<input v-model="tag.key" required :disabled="busy || !!loadError" /></label>
                  <label>Tag value<input v-model="tag.value" :disabled="busy || !!loadError" /></label>
                  <button type="button" :disabled="busy || !!loadError" @click="tags.splice(index, 1)">Remove tag</button>
                </div>
                <Notice v-if="tagError" tone="error">{{ tagError }}</Notice>
              </section>
              <button class="primary" :disabled="busy || !!loadError || !validRoleDuration(duration) || !!tagError">Save role settings</button>
            </form>
            <form v-else-if="tab === 'trust'" class="panel-form" @submit.prevent="apply('trust')">
              <label>Trust policy JSON<textarea v-model="trust" required rows="12" spellcheck="false" :disabled="busy || !!loadError" /></label>
              <button class="primary" :disabled="busy || !!loadError || !validPolicy(trust)">Save trust policy</button>
            </form>
            <IamPolicies v-else :connection="connection" kind="Role" :name="role.name" />
          </div>
        </FadeRegion>
      </template>
    </template>
    <Modal v-if="creating" title="Create IAM role" wide @close="!busy && (creating = undefined)">
      <form class="modal-form" @submit.prevent="create">
        <Notice v-if="error" tone="error">{{ error }}</Notice>
        <div class="form-grid">
          <label>Role name<input v-model="creating.name" required :disabled="busy" /></label>
          <label>IAM path<input v-model="creating.path" required :disabled="busy" /></label>
        </div>
        <label>Maximum session duration in seconds<input v-model.number="creating.duration" type="number" min="3600" max="43200" required :disabled="busy" /></label>
        <label>Trust policy JSON<textarea v-model="creating.trust" required rows="9" spellcheck="false" :disabled="busy" /></label>
        <div class="modal-actions">
          <button type="button" :disabled="busy" @click="creating = undefined">Cancel</button>
          <button class="primary" :disabled="busy || !creating.name.trim() || !validRoleDuration(creating.duration) || !validPolicy(creating.trust)">Create role</button>
        </div>
      </form>
    </Modal>
    <ConfirmDialog v-if="removing" :title="`Delete ${selected}?`" description="This removes the role and its trust relationship from the provider."
      action="Delete role" danger @close="removing = false" @confirm="apply('delete')" />
  </div>
</template>
