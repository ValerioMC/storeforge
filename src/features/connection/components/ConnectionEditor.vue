<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue'
import { Eye, EyeOff, ShieldCheck } from 'lucide-vue-next'
import { errorMessage } from '../../../common/tauri'
import AppSelect from '../../../components/ui/AppSelect.vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import { connectionsApi } from '../api/connections'
import { capabilityNames, endpointSettings, identityNames, providerNames, serviceIdentities,
  type Connection, type ConnectionCheck, type ConnectionDraft, type IdentityKind, type Provider,
  type ServiceDraft, type ServiceIdentityKind } from '../model/connection'
import { parseProfile } from '../model/profile-import'
import ServiceIdentityFields from './ServiceIdentityFields.vue'
import { useConnections } from '../store/connections'

const props = defineProps<{ profile?: Connection }>()
const emit = defineEmits<{ close: []; saved: [] }>()
const connections = useConnections()
const name = ref(props.profile?.name ?? '')
const provider = ref<Provider>(props.profile?.provider ?? 'ceph')
const endpoint = ref(props.profile?.endpoint ?? '')
const region = ref(props.profile?.region ?? 'us-east-1')
const pathStyle = ref(props.profile?.pathStyle ?? true)
const known = ref(props.profile?.knownBuckets.join('\n') ?? '')
const accessKey = ref('')
const secretKey = ref('')
const sessionToken = ref('')
const reveal = ref(false)
const settings = reactive({ ...endpointSettings(), ...props.profile?.settings })
const standardTags = ref(settings.standardTags.join(', '))
const administrativeTags = ref(settings.administrativeTags.join(', '))
const reconciliations = reactive<Record<ServiceIdentityKind, ServiceDraft | null>>({
  adminOps: null, runtime: null, supervision: null, cephAdmin: null,
})
const importInput = ref<HTMLInputElement>()
const busy = ref<'test' | 'save' | null>(null)
const result = ref<ConnectionCheck>()
const error = ref('')
let draftVersion = 0
const providerOptions = (Object.keys(providerNames) as Provider[]).map(value => ({ value, label: providerNames[value] }))
const addressingOptions = [
  { value: true, label: 'Path style' },
  { value: false, label: 'Virtual hosted' },
]
const updatesCredentials = computed(() => !props.profile || Boolean(accessKey.value || secretKey.value || sessionToken.value))
const valid = computed(() => Boolean(name.value.trim() && region.value.trim() && (endpoint.value.trim() || provider.value === 'aws') &&
  (!updatesCredentials.value || (accessKey.value.trim() && secretKey.value.trim()))))

watch(provider, value => { pathStyle.value = value !== 'aws' }, { flush: 'sync' })
watch([name, provider, endpoint, region, pathStyle, known, accessKey, secretKey, sessionToken, settings,
  standardTags, administrativeTags, reconciliations], () => { draftVersion++; result.value = undefined; error.value = '' }, { flush: 'sync' })

function labels(value: string): string[] {
  return [...new Set(value.split(/[,\n]/).map(tag => tag.trim()).filter(Boolean))]
}

function draft(validateS3: boolean): ConnectionDraft {
  const reconcile: ConnectionDraft['reconcile'] = {}
  for (const identity of serviceIdentities) {
    const change = reconciliations[identity]
    if (change) reconcile[identity] = change
  }
  return {
    id: props.profile?.id ?? null, name: name.value, provider: provider.value, endpoint: endpoint.value, region: region.value,
    pathStyle: pathStyle.value, knownBuckets: known.value.split(/[,\n]/).map(value => value.trim()).filter(Boolean),
    settings: { ...settings, standardTags: labels(standardTags.value), administrativeTags: labels(administrativeTags.value) },
    validateS3,
    reconcile,
    credentials: updatesCredentials.value ? { accessKey: accessKey.value.trim(), secretKey: secretKey.value, sessionToken: sessionToken.value || null } : null,
  }
}

async function run(action: 'test' | 'save'): Promise<void> {
  if (!valid.value || busy.value) return
  error.value = ''
  const version = draftVersion
  const validate = action === 'test' || result.value !== undefined
  busy.value = action
  try {
    if (action === 'test') {
      const response = await connectionsApi.test(draft(validate))
      if (version === draftVersion) result.value = response
      return
    }
    await connections.save(draft(validate))
    emit('saved')
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
    standardTags.value = settings.standardTags.join(', ')
    administrativeTags.value = settings.administrativeTags.join(', ')
  } catch (failure) {
    error.value = errorMessage(failure)
  }
}
</script>

<template>
  <Modal :title="profile ? 'Edit connection' : 'New connection'" wide @close="!busy && emit('close')">
    <form class="modal-form" @submit.prevent="run('save')">
      <div class="form-grid">
        <div v-if="!profile" class="span-two panel-actions">
          <input ref="importInput" type="file" accept=".json,application/json" hidden @change="importProfile" />
          <button type="button" :disabled="!!busy" @click="importInput?.click()">Import profile settings</button>
        </div>
        <label>Connection name<input v-model="name" placeholder="e.g. Production RGW" required autofocus /></label>
        <label>Provider<AppSelect v-model="provider" :options="providerOptions" /></label>
        <label class="span-two">Endpoint URL<input v-model="endpoint" type="url" :required="provider !== 'aws'" :placeholder="provider === 'aws' ? 'Use the AWS endpoint for this region' : 'https://storage.example.com'" /></label>
        <label>Signing region<input v-model="region" required spellcheck="false" /></label>
        <label>Addressing<AppSelect v-model="pathStyle" :options="addressingOptions" /></label>
      </div>
      <fieldset>
        <legend>Credentials</legend>
        <p v-if="profile" class="muted">Leave the fields blank to keep the saved credentials.</p>
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
        <label>Bucket names
          <textarea v-model="known" rows="2" placeholder="One bucket per line" spellcheck="false" />
          <small>Use this when the key cannot list every bucket on the endpoint.</small>
        </label>
      </details>
      <details class="disclosure">
        <summary>Endpoint settings</summary>
        <div class="form-grid">
          <label>Tenant<input v-model="settings.tenant" spellcheck="false" /></label>
          <label>Account<input v-model="settings.account" spellcheck="false" /></label>
          <label v-if="provider === 'ceph'">Ceph realm<input v-model="settings.realm" spellcheck="false" /></label>
          <label class="check-row"><input v-model="settings.defaultEndpoint" type="checkbox" /> Default endpoint</label>
          <label>Standard tags<input v-model="standardTags" placeholder="Comma separated" /></label>
          <label>Administrative tags<input v-model="administrativeTags" placeholder="Comma separated" /></label>
          <label class="span-two">Additional CA certificates <small>PEM</small><textarea v-model="settings.caCertificate" rows="3" spellcheck="false" /></label>
        </div>
      </details>
      <details class="disclosure">
        <summary>Service identities</summary>
        <ServiceIdentityFields v-for="identity in serviceIdentities" :key="identity" v-model="reconciliations[identity]"
          :identity="identity" :saved="profile?.services?.[identity]" :available="identity === 'runtime' || provider === 'ceph'" />
      </details>
      <p class="credential-note"><ShieldCheck :size="16" /> Saved credentials use your system credential store.</p>
      <Notice v-if="error" tone="error">{{ error }}</Notice>
      <Notice v-if="result" :tone="result.capabilities.s3 === 'detected' ? 'success' : 'info'">
        <span v-if="result.bucketCount !== null">{{ result.bucketCount }} {{ result.bucketCount === 1 ? 'bucket' : 'buckets' }} available.</span>
        <ul class="credential-checks">
          <li v-for="(state, identity) in result.capabilities" :key="identity">{{ identityLabel(identity) }} · {{ capabilityNames[state] }}</li>
        </ul>
      </Notice>
      <div class="modal-actions">
        <button type="button" class="push-left" :disabled="!valid || !!busy" @click="run('test')">{{ busy === 'test' ? 'Testing…' : 'Test connection' }}</button>
        <button type="button" :disabled="!!busy" @click="emit('close')">Cancel</button>
        <button class="primary" :disabled="!valid || !!busy">{{ busy === 'save' ? 'Saving…' : 'Save connection' }}</button>
      </div>
    </form>
  </Modal>
</template>
