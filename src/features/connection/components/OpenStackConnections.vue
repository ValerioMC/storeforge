<script setup lang="ts">
import { computed, onMounted, reactive, ref, useId, watch } from 'vue'
import { Cloud, Pencil, Plus, Server, Trash2 } from 'lucide-vue-next'
import { errorMessage } from '../../../common/tauri'
import AppSelect from '../../../components/ui/AppSelect.vue'
import ConfirmDialog from '../../../components/ui/ConfirmDialog.vue'
import FadeRegion from '../../../components/ui/FadeRegion.vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import { openStackApi } from '../api/openstack'
import type { Connection } from '../model/connection'
import { needsOpenStackSignIn, onlyChoice, type OpenStackAuthentication, type OpenStackCloud,
  type OpenStackCloudSettings, type OpenStackCloudStatus, type OpenStackProjectAccess } from '../model/openstack'
import { useConnections } from '../store/connections'

const emit = defineEmits<{ browse: [profile: Connection] }>()
const heading = useId()
const connections = useConnections()
const clouds = ref<OpenStackCloudStatus[]>([])
const cloudId = ref('')
const configuredCloud = computed(() => clouds.value.find(status => status.cloud.id === cloudId.value))
const cloudOptions = computed(() => clouds.value.map(({ cloud }) => ({ value: cloud.id, label: cloud.name })))
const profiles = computed(() => connections.profiles.filter(profile => profile.settings?.openstack?.cloudId === cloudId.value))
const busy = ref(false)
const error = ref('')
const editing = ref<string | null>()
const forgetting = ref(false)
const settings = reactive<OpenStackCloudSettings>({ name: '', authUrl: '', defaultDomain: 'Default', managedEndpointsOnly: false })
const method = ref<OpenStackAuthentication['method']>('password')
const username = ref('')
const domain = ref('')
const credential = ref('')
const projectId = ref('')
const access = ref<OpenStackProjectAccess>()
const targetId = ref('')
const accessKey = ref('')
const methodOptions = [
  { value: 'password' as const, label: 'Username and password' },
  { value: 'token' as const, label: 'Primary Keystone token' },
]
const projectOptions = computed(() => (configuredCloud.value?.session?.projects ?? []).map(project => ({
  value: project.id, label: `${project.name} (${project.id})`,
})))
const allowedTargets = computed(() => access.value?.targets.filter(target => target.allowed) ?? [])
const targetOptions = computed(() => allowedTargets.value.map(target => ({
  value: target.id, label: `${target.region} · ${target.interface} · ${target.endpoint}`,
})))
const keyOptions = computed(() => (access.value?.accessKeys ?? []).map(key => ({ value: key, label: key })))
const rejectedRegions = computed(() => [...new Set(access.value?.targets.filter(target => !target.allowed).map(target => target.region))])
const target = computed(() => allowedTargets.value.find(target => target.id === targetId.value))

onMounted(() => { void run(reloadClouds) })

watch(() => configuredCloud.value?.session?.expiresAt, (expiresAt, _, cleanup) => {
  if (!expiresAt) return
  const delay = Math.max(0, Date.parse(expiresAt) - Date.now())
  if (delay > 2_147_483_647) return
  const expiry = window.setTimeout(() => { void run(reloadClouds) }, delay)
  cleanup(() => window.clearTimeout(expiry))
})

async function reloadClouds(): Promise<void> {
  clouds.value = await openStackApi.list()
  if (!configuredCloud.value) selectCloud(clouds.value[0]?.cloud.id ?? '')
}

function selectCloud(id: string): void {
  cloudId.value = id
  username.value = ''
  domain.value = ''
  setMethod('password')
  projectId.value = ''
  resetAccess()
  error.value = ''
}

function setMethod(value: OpenStackAuthentication['method']): void {
  method.value = value
  credential.value = ''
}

function resetAccess(): void {
  access.value = undefined
  targetId.value = ''
  accessKey.value = ''
}

function edit(cloud?: OpenStackCloud): void {
  editing.value = cloud?.id ?? null
  settings.name = cloud?.name ?? ''
  settings.authUrl = cloud?.authUrl ?? ''
  settings.defaultDomain = cloud?.defaultDomain ?? 'Default'
  settings.managedEndpointsOnly = cloud?.managedEndpointsOnly ?? false
}

async function run(operation: () => Promise<void>): Promise<void> {
  if (busy.value) return
  error.value = ''
  busy.value = true
  try {
    await operation()
  } catch (failure) {
    error.value = errorMessage(failure)
    if (needsOpenStackSignIn(failure)) {
      resetAccess()
      try {
        await Promise.all([reloadClouds(), connections.refreshProfiles()])
      } catch (refreshFailure) {
        error.value = errorMessage(refreshFailure)
      }
    }
  } finally {
    busy.value = false
  }
}

async function saveCloud(): Promise<void> {
  const id = editing.value
  if (id === undefined) return
  await run(async () => {
    const cloud = await openStackApi.save(id, { ...settings })
    editing.value = undefined
    selectCloud(cloud.id)
    await Promise.all([reloadClouds(), connections.refreshProfiles()])
  })
}

async function signIn(): Promise<void> {
  const id = cloudId.value
  const authentication: OpenStackAuthentication = method.value === 'token'
    ? { method: 'token', token: credential.value }
    : { method: 'password', username: username.value, domain: domain.value, password: credential.value }
  credential.value = ''
  await run(async () => {
    const session = await openStackApi.authenticate(id, authentication)
    await reloadClouds()
    const project = onlyChoice(session.projects)
    if (project) await discover(project.id)
  })
}

async function discover(id: string): Promise<void> {
  projectId.value = id
  resetAccess()
  const result = await openStackApi.discover(cloudId.value, id)
  access.value = result
  targetId.value = onlyChoice(result.targets.filter(target => target.allowed))?.id ?? ''
  accessKey.value = onlyChoice(result.accessKeys) ?? ''
}

async function activate(): Promise<void> {
  const discovery = access.value
  if (!discovery || !target.value || !accessKey.value) return
  await run(async () => {
    const profile = await openStackApi.activate(cloudId.value, {
      discoveryId: discovery.id, targetId: targetId.value, accessKey: accessKey.value,
    })
    await connections.refreshProfiles()
    emit('browse', profile)
  })
}

async function disconnect(forget = false): Promise<void> {
  await run(async () => {
    const id = cloudId.value
    await openStackApi.disconnect(id, forget)
    selectCloud(forget ? '' : id)
    await Promise.all([reloadClouds(), connections.refreshProfiles()])
  })
}

function forgetCloud(): void {
  forgetting.value = false
  void disconnect(true)
}
</script>

<template>
  <section class="openstack" :aria-labelledby="heading">
    <header class="settings-toolbar">
      <div>
        <h2 :id="heading">OpenStack</h2>
        <p>Open a project’s S3 storage using an existing EC2 credential.</p>
      </div>
      <button type="button" :disabled="busy" @click="edit()"><Plus :size="16" /> Add cloud</button>
    </header>

    <Notice v-if="error && editing === undefined" tone="error">{{ error }}</Notice>
    <div v-if="configuredCloud" class="settings-sheet">
      <div class="settings-row cloud-row">
        <span class="file-icon"><Cloud :size="19" /></span>
        <div class="settings-description">
          <AppSelect :model-value="cloudId" :options="cloudOptions" aria-label="OpenStack cloud"
            :disabled="busy" @update:model-value="selectCloud" />
          <small>{{ configuredCloud.cloud.authUrl }}</small>
        </div>
        <button class="icon-button subtle" type="button" aria-label="Edit cloud" :disabled="busy" @click="edit(configuredCloud.cloud)"><Pencil :size="16" /></button>
        <button class="icon-button subtle" type="button" aria-label="Forget cloud" :disabled="busy" @click="forgetting = true"><Trash2 :size="16" /></button>
      </div>
      <FadeRegion>
        <div v-if="configuredCloud.session" :key="`${cloudId}:project`" class="openstack-form panel-form">
          <div class="settings-toolbar">
            <div class="settings-description">
              <strong>{{ configuredCloud.session.identity }}</strong>
              <small>Keystone session expires {{ new Date(configuredCloud.session.expiresAt).toLocaleString() }}</small>
            </div>
            <button type="button" :disabled="busy" @click="disconnect()">Disconnect</button>
          </div>
          <div v-if="profiles.length" class="active-projects">
            <div v-for="profile in profiles" :key="profile.id" class="settings-row">
              <Server :size="16" />
              <div class="settings-description">
                <strong>{{ profile.settings?.openstack?.projectName }} · {{ profile.region }}</strong>
                <small>{{ profile.endpoint }}</small>
              </div>
              <button type="button" :disabled="busy" @click="emit('browse', profile)">Open workspace</button>
            </div>
          </div>
          <p v-if="!projectOptions.length">Keystone returned no active projects for this user.</p>
          <template v-else>
            <label>Project
              <AppSelect :model-value="projectId" :options="projectOptions" aria-label="OpenStack project"
                placeholder="Choose a project" :disabled="busy" @update:model-value="run(() => discover($event))" />
            </label>
            <template v-if="access">
              <div class="form-grid">
                <label>S3 region
                  <AppSelect v-model="targetId" :options="targetOptions" aria-label="S3 region"
                    placeholder="Choose a region and endpoint" :disabled="busy" />
                </label>
                <label>EC2 credential
                  <AppSelect v-model="accessKey" :options="keyOptions" aria-label="EC2 credential"
                    placeholder="Choose an existing key" :disabled="busy" />
                </label>
              </div>
              <small v-if="target" class="target-endpoint">{{ target.endpoint }}</small>
              <Notice v-if="rejectedRegions.length">Unavailable under this cloud’s managed-endpoint policy: {{ rejectedRegions.join(', ') }}.</Notice>
              <Notice v-if="!targetOptions.length">This project has no eligible regional S3 endpoints.</Notice>
              <Notice v-if="!keyOptions.length">Keystone returned no existing EC2 credentials for this project.</Notice>
              <div class="panel-actions">
                <button class="primary" type="button" :disabled="busy || !target || !accessKey" @click="activate">Open project workspace</button>
              </div>
            </template>
            <span v-else-if="busy" role="status">Loading project access…</span>
          </template>
        </div>
        <form v-else :key="`${cloudId}:signin`" class="openstack-form panel-form" @submit.prevent="signIn">
          <Notice v-if="configuredCloud.reconnectRequired">Sign in again to reopen this cloud’s workspaces.</Notice>
          <label>Authentication
            <AppSelect :model-value="method" :options="methodOptions" aria-label="Keystone authentication"
              :disabled="busy" @update:model-value="setMethod" />
          </label>
          <div v-if="method === 'password'" class="form-grid">
            <label>Username<input v-model="username" required :disabled="busy" autocomplete="username" /></label>
            <label>User domain<input v-model="domain" :placeholder="configuredCloud.cloud.defaultDomain" :disabled="busy" autocomplete="off" /></label>
          </div>
          <label>{{ method === 'password' ? 'Password' : 'Primary token' }}
            <input v-model="credential" type="password" required :disabled="busy" :autocomplete="method === 'password' ? 'current-password' : 'off'" />
          </label>
          <div class="panel-actions"><button class="primary" :disabled="busy">{{ busy ? 'Signing in…' : 'Sign in to Keystone' }}</button></div>
        </form>
      </FadeRegion>
    </div>
    <Modal v-if="editing !== undefined" :title="editing ? 'Edit OpenStack cloud' : 'Add OpenStack cloud'" @close="!busy && (editing = undefined)">
      <form class="modal-form" @submit.prevent="saveCloud">
        <Notice v-if="error" tone="error">{{ error }}</Notice>
        <label>Cloud name<input v-model="settings.name" required :disabled="busy" autocomplete="off" /></label>
        <label>Keystone v3 URL<input v-model="settings.authUrl" type="url" required :disabled="busy" placeholder="https://identity.example.com/v3" /></label>
        <label>Default user domain<input v-model="settings.defaultDomain" :disabled="busy" autocomplete="off" /></label>
        <label class="check-label"><input v-model="settings.managedEndpointsOnly" type="checkbox" :disabled="busy" /> Limit access to saved S3 endpoints</label>
        <p v-if="editing && configuredCloud?.session">Saving disconnects the current Keystone session.</p>
        <div class="modal-actions">
          <button type="button" :disabled="busy" @click="editing = undefined">Cancel</button>
          <button class="primary" :disabled="busy">Save cloud</button>
        </div>
      </form>
    </Modal>
    <ConfirmDialog v-if="forgetting && configuredCloud" title="Forget OpenStack cloud?"
      :description="`Remove ${configuredCloud.cloud.name} and its sessions from this device. Your EC2 keys will stay in Keystone.`"
      action="Forget cloud" danger @close="forgetting = false" @confirm="forgetCloud" />
  </section>
</template>

<style scoped>
.openstack { display: grid; gap: 10px; margin-top: 20px; }
.openstack h2 { font-size: 16px; }
.openstack header p { margin-top: 3px; }
.cloud-row .settings-description small, .active-projects small { overflow-wrap: anywhere; }
.openstack-form { padding: 13px; }
.active-projects { border: 1px solid var(--line); border-radius: var(--radius); }
.active-projects .settings-row { flex-wrap: wrap; }
.target-endpoint { color: var(--muted); overflow-wrap: anywhere; }
</style>
