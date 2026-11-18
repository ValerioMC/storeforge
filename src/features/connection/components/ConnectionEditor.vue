<script setup lang="ts">
import { computed, nextTick, reactive, ref, watch } from 'vue'
import { Check, ChevronRight, Eye, EyeOff, ShieldCheck } from 'lucide-vue-next'
import { errorMessage } from '../../../common/tauri'
import AppSelect from '../../../components/ui/AppSelect.vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import { connectionsApi } from '../api/connections'
import { capabilityNames, endpointSettings, identityNames, providerNames, serviceIdentities, validCoordinates,
  type Connection, type ConnectionCheck, type ConnectionDraft, type IdentityKind, type Provider,
  type ServiceDraft, type ServiceIdentityKind, type ProfileFields } from '../model/connection'
import { discardSetup, pausedSetup, pauseSetup, setupStages, type SetupStage } from '../model/setup-draft'
import { parseProfile } from '../model/profile-import'
import ServiceIdentityFields from './ServiceIdentityFields.vue'
import { useConnections } from '../store/connections'

const props = defineProps<{ profile?: Connection; resume?: boolean }>()
const emit = defineEmits<{ close: []; saved: [profile: Connection] }>()
const connections = useConnections()
const paused = props.resume ? pausedSetup() : undefined
const initial = props.profile ?? paused?.profile
const stage = ref<SetupStage>(paused?.stage ?? 'endpoint')
const stageHeading = ref<HTMLElement>()
const name = ref(initial?.name ?? '')
const provider = ref<Provider>(initial?.provider ?? 'ceph')
const endpoint = ref(initial?.endpoint ?? '')
const region = ref(initial?.region ?? 'us-east-1')
const pathStyle = ref(initial?.pathStyle ?? true)
const known = ref(initial?.knownBuckets.join('\n') ?? '')
const accessKey = ref('')
const secretKey = ref('')
const sessionToken = ref('')
const reveal = ref(false)
const settings = reactive({ ...endpointSettings(), ...initial?.settings })
const latitude = ref<number | ''>(settings.coordinates?.latitude ?? '')
const longitude = ref<number | ''>(settings.coordinates?.longitude ?? '')
const coordinatesValid = computed(() => (latitude.value === '' && longitude.value === '') || validCoordinates(latitude.value, longitude.value))
const standardTags = ref(settings.standardTags.join(', '))
const administrativeTags = ref(settings.administrativeTags.join(', '))
const reconciliations = reactive<Record<ServiceIdentityKind, ServiceDraft | null>>({
  adminOps: null, runtime: null, supervision: null, cephAdmin: null,
})
const importInput = ref<HTMLInputElement>()
const busy = ref<'test' | 'save' | null>(null)
const result = ref<ConnectionCheck>()
const workspaceChoice = computed({
  get: () => settings.defaultWorkspace ?? '',
  set: (value: string) => {
    settings.defaultWorkspace = value === 'browser' || value === 'manager' || value === 'cephAdmin' || value === 'admin' ? value : null
  },
})
const workspaceOptions = computed(() => [
  { value: '', label: 'Choose an available workspace' },
  ...(settings.browserEnabled ? [{ value: 'browser', label: 'Browser' }] : []),
  ...(settings.managerEnabled ? [{ value: 'manager', label: 'Manager' }] : []),
  ...(provider.value === 'ceph' ? [{ value: 'cephAdmin', label: 'Ceph Admin' }] : []),
  { value: 'admin', label: 'Admin' },
])
const stageIndex = computed(() => setupStages.findIndex(item => item.value === stage.value))
const saveBlocked = computed(() => (settings.browserEnabled || settings.managerEnabled) &&
  (result.value?.capabilities.s3 === 'denied' || result.value?.capabilities.s3 === 'unavailable'))
const error = ref('')
let draftVersion = 0
const providerOptions = (Object.keys(providerNames) as Provider[]).map(value => ({ value, label: providerNames[value] }))
const addressingOptions = [
  { value: true, label: 'Path style' },
  { value: false, label: 'Virtual hosted' },
]
const updatesCredentials = computed(() => !props.profile || Boolean(accessKey.value || secretKey.value || sessionToken.value))
const validEndpoint = computed(() => Boolean(name.value.trim() && region.value.trim() &&
  (endpoint.value.trim() || provider.value === 'aws') && coordinatesValid.value))
const validCredentials = computed(() => (!updatesCredentials.value || Boolean(accessKey.value.trim() && secretKey.value.trim())) &&
  serviceIdentities.every(identity => {
    const change = reconciliations[identity]
    if (!change?.enabled) return true
    return change.credentials ? Boolean(change.credentials.accessKey && change.credentials.secretKey.trim()) : !!props.profile?.services?.[identity]
  }))

watch(provider, value => { pathStyle.value = value !== 'aws' }, { flush: 'sync' })
watch(workspaceOptions, options => {
  if (!options.some(option => option.value === workspaceChoice.value)) workspaceChoice.value = ''
})
watch([name, provider, endpoint, region, pathStyle, known, accessKey, secretKey, sessionToken, settings,
  standardTags, administrativeTags, latitude, longitude, reconciliations], () => { draftVersion++; result.value = undefined; error.value = '' }, { flush: 'sync' })

function labels(value: string): string[] {
  return [...new Set(value.split(/[,\n]/).map(tag => tag.trim()).filter(Boolean))]
}

function fields(): ProfileFields {
  return {
    name: name.value, provider: provider.value, endpoint: endpoint.value, region: region.value,
    pathStyle: pathStyle.value, knownBuckets: labels(known.value),
    settings: { ...settings, coordinates: typeof latitude.value === 'number' && typeof longitude.value === 'number'
      ? { latitude: latitude.value, longitude: longitude.value } : null, standardTags: labels(standardTags.value), administrativeTags: labels(administrativeTags.value) },
  }
}

function draft(validateS3: boolean): ConnectionDraft {
  const reconcile: ConnectionDraft['reconcile'] = {}
  for (const identity of serviceIdentities) {
    const change = reconciliations[identity]
    if (change) reconcile[identity] = change
  }
  return {
    ...fields(),
    id: props.profile?.id ?? null,
    validateS3,
    reconcile,
    credentials: updatesCredentials.value ? { accessKey: accessKey.value.trim(), secretKey: secretKey.value, sessionToken: sessionToken.value || null } : null,
  }
}

function goTo(value: SetupStage): void {
  stage.value = value
  void nextTick(() => stageHeading.value?.focus())
}

function previous(): void {
  const target = setupStages[stageIndex.value - 1]
  if (target) goTo(target.value)
}

function close(): void {
  if (busy.value) return
  if (!props.profile) pauseSetup(fields(), stage.value)
  emit('close')
}

function discard(): void {
  if (!props.profile) discardSetup()
  emit('close')
}

async function advance(): Promise<void> {
  if (stage.value === 'endpoint' && validEndpoint.value) goTo('workspaces')
  else if (stage.value === 'workspaces') goTo('credentials')
  else if (stage.value === 'credentials' && result.value && !saveBlocked.value) goTo('review')
  else if (stage.value === 'review') await run('save')
}

async function run(action: 'test' | 'save'): Promise<void> {
  if (!validEndpoint.value || !validCredentials.value || busy.value) return
  error.value = ''
  const version = draftVersion
  busy.value = action
  try {
    if (action === 'test') {
      result.value = undefined
      const response = await connectionsApi.test(draft(true))
      if (version === draftVersion) result.value = response
      return
    }
    const saved = await connections.save(draft(true))
    if (!props.profile) discardSetup()
    emit('saved', saved)
  } catch (failure) {
    if (action === 'save' || version === draftVersion) error.value = errorMessage(failure)
  } finally {
    busy.value = null
  }
}

function identityLabel(value: IdentityKind): string {
  return identityNames[value]
}

async function importProfile(event: Event): Promise<void> {
  const input = event.currentTarget as HTMLInputElement
  const file = input.files?.[0]
  input.value = ''
  if (!file) return
  try {
    if (file.size > 1_048_576) throw new Error('The profile must be smaller than 1 MiB.')
    const imported = parseProfile(await file.text())
    name.value = imported.name
    provider.value = imported.provider
    endpoint.value = imported.endpoint
    region.value = imported.region
    pathStyle.value = imported.pathStyle
    known.value = imported.knownBuckets.join('\n')
    Object.assign(settings, imported.settings)
    latitude.value = settings.coordinates?.latitude ?? ''
    longitude.value = settings.coordinates?.longitude ?? ''
    standardTags.value = settings.standardTags.join(', ')
    administrativeTags.value = settings.administrativeTags.join(', ')
  } catch (failure) {
    error.value = errorMessage(failure)
  }
}
</script>

<template>
  <Modal :title="profile ? 'Edit connection' : 'Set up a connection'" wide @close="close">
    <form class="modal-form setup-form" @submit.prevent="advance">
      <ol class="setup-progress" aria-label="Setup progress">
        <li v-for="(item, index) in setupStages" :key="item.value" :aria-current="stage === item.value ? 'step' : undefined">
          <span class="setup-step-number"><Check v-if="stageIndex > index" :size="13" /><template v-else>{{ index + 1 }}</template></span>
          {{ item.label }}
          <ChevronRight v-if="index < setupStages.length - 1" :size="13" class="setup-divider" />
        </li>
      </ol>

      <div class="setup-stage">
        <h3 ref="stageHeading" tabindex="-1">{{ setupStages[stageIndex]?.label }}</h3>
        <section v-show="stage === 'endpoint'" class="panel-form">
          <div v-if="!profile" class="panel-actions">
            <input ref="importInput" type="file" accept=".json,application/json" hidden @change="importProfile" />
            <button type="button" :disabled="!!busy" @click="importInput?.click()">Import settings</button>
          </div>
          <div class="form-grid">
            <label>Connection name<input v-model="name" placeholder="e.g. Production RGW" autofocus /></label>
            <label>Provider<AppSelect v-model="provider" :options="providerOptions" /></label>
            <label class="span-two">Endpoint URL<input v-model="endpoint" :type="stage === 'endpoint' ? 'url' : 'text'" :placeholder="provider === 'aws' ? 'Use the AWS endpoint for this region' : 'https://storage.example.com'" /></label>
            <label>Signing region<input v-model="region" spellcheck="false" /></label>
            <label>Addressing<AppSelect v-model="pathStyle" :options="addressingOptions" /></label>
          </div>
          <details class="disclosure">
            <summary>Tenant, realm, TLS and location</summary>
            <div class="form-grid">
              <label>Tenant<input v-model="settings.tenant" spellcheck="false" /></label>
              <label>Account<input v-model="settings.account" spellcheck="false" /></label>
              <label v-if="provider === 'ceph'">Ceph realm<input v-model="settings.realm" spellcheck="false" /></label>
              <label>Latitude <small>Optional</small><input v-model.number="latitude" type="number" min="-90" max="90" step="any" aria-describedby="coordinates-help" /></label>
              <label>Longitude <small>Optional</small><input v-model.number="longitude" type="number" min="-180" max="180" step="any" aria-describedby="coordinates-help" /></label>
              <p id="coordinates-help" class="span-two">{{ coordinatesValid ? 'Provide both coordinates to pin this endpoint on the infrastructure map.' : 'Enter a latitude from −90 to 90 and a longitude from −180 to 180.' }}</p>
              <label class="span-two">Additional CA certificates <small>PEM</small><textarea v-model="settings.caCertificate" rows="3" spellcheck="false" /></label>
            </div>
          </details>
        </section>

        <section v-show="stage === 'workspaces'" class="panel-form">
          <div class="setup-workspace-options">
            <label class="check-row"><input v-model="settings.browserEnabled" type="checkbox" /> Browser</label>
            <p>Browse files, work with versions, and manage objects.</p>
            <label class="check-row"><input v-model="settings.managerEnabled" type="checkbox" /> Manager</label>
            <p>Manage buckets, account access, and storage tools.</p>
          </div>
          <div class="form-grid">
            <label>Default workspace<AppSelect v-model="workspaceChoice" :options="workspaceOptions" /></label>
            <label class="check-row"><input v-model="settings.defaultEndpoint" type="checkbox" /> Default endpoint</label>
            <label>Standard tags<input v-model="standardTags" placeholder="Comma separated" /></label>
            <label>Administrative tags<input v-model="administrativeTags" placeholder="Comma separated" /></label>
          </div>
          <p v-if="provider === 'ceph'" class="muted">Ceph Admin becomes available when an administrative identity passes its check.</p>
        </section>

        <section v-show="stage === 'credentials'" class="panel-form">
          <fieldset :disabled="!!busy">
            <legend>S3 credentials</legend>
            <p v-if="profile" class="muted">Leave blank to use the saved credentials.</p>
            <div class="form-grid">
              <label>Access key ID<input v-model="accessKey" autocomplete="off" autocapitalize="off" spellcheck="false" /></label>
              <label>Secret access key
                <span class="input-action"><input v-model="secretKey" :type="reveal ? 'text' : 'password'" autocomplete="off" spellcheck="false" />
                  <button type="button" class="icon-button subtle" :aria-label="reveal ? 'Hide secret' : 'Show secret'" @click="reveal = !reveal"><EyeOff v-if="reveal" :size="16" /><Eye v-else :size="16" /></button>
                </span>
              </label>
              <label class="span-two">Session token <small>Optional</small><input v-model="sessionToken" type="password" autocomplete="off" /></label>
            </div>
          </fieldset>
          <details class="disclosure">
            <summary>Restricted bucket access</summary>
            <label>Known bucket names<textarea v-model="known" rows="2" placeholder="One bucket per line" spellcheck="false" :disabled="!!busy" />
              <small>Supply bucket names when the key has access to specific buckets.</small>
            </label>
          </details>
          <details class="disclosure">
            <summary>Service identities</summary>
            <fieldset class="service-fields" :disabled="!!busy">
              <ServiceIdentityFields v-for="identity in serviceIdentities" :key="identity" v-model="reconciliations[identity]"
                :identity="identity" :saved="profile?.services?.[identity]" :available="identity === 'runtime' || provider === 'ceph'" />
            </fieldset>
          </details>
          <p class="credential-note"><ShieldCheck :size="16" /> Saved credentials use your system credential store.</p>
        </section>

        <section v-if="stage === 'review'" class="panel-form">
          <dl class="setup-summary">
            <div><dt>Connection</dt><dd>{{ name }}</dd></div>
            <div><dt>Endpoint</dt><dd>{{ endpoint || 'AWS regional endpoint' }}</dd></div>
            <div><dt>Region</dt><dd>{{ region }}</dd></div>
            <div><dt>Workspaces</dt><dd>{{ [settings.browserEnabled && 'Browser', settings.managerEnabled && 'Manager'].filter(Boolean).join(', ') || 'Admin only' }}</dd></div>
          </dl>
          <p v-if="result?.capabilities.s3 === 'unknown'" class="muted">The endpoint could not be verified. Check it again once it is reachable.</p>
        </section>
      </div>

      <Notice v-if="error" tone="error">{{ error }}</Notice>
      <Notice v-if="result && (stage === 'credentials' || stage === 'review')" :tone="saveBlocked ? 'error' : result.capabilities.s3 === 'detected' ? 'success' : 'info'">
        <span v-if="result.bucketCount !== null">{{ result.bucketCount }} {{ result.bucketCount === 1 ? 'bucket' : 'buckets' }} available.</span>
        <span v-if="saveBlocked">Check the key, endpoint, or known buckets before continuing.</span>
        <span class="credential-checks">
          <span v-for="(state, identity) in result.capabilities" :key="identity" class="chip">{{ identityLabel(identity) }} · {{ capabilityNames[state] }}</span>
        </span>
      </Notice>
      <div class="modal-actions">
        <button type="button" class="subtle push-left" :disabled="!!busy" @click="discard">{{ profile ? 'Cancel' : 'Discard setup' }}</button>
        <button v-if="!profile" type="button" :disabled="!!busy" @click="close">Pause setup</button>
        <button v-if="stage !== 'endpoint'" type="button" :disabled="!!busy" @click="previous">Back</button>
        <button v-if="stage === 'credentials'" type="button" :disabled="!validCredentials || !!busy" @click="run('test')">{{ busy === 'test' ? 'Checking…' : 'Check credentials' }}</button>
        <button class="primary" :disabled="!!busy || (stage === 'endpoint' && !validEndpoint) || (stage === 'credentials' && (!result || saveBlocked))">
          {{ stage === 'review' ? (busy === 'save' ? 'Applying…' : 'Apply connection') : stage === 'credentials' ? 'Review setup' : 'Continue' }}
        </button>
      </div>
    </form>
  </Modal>
</template>
