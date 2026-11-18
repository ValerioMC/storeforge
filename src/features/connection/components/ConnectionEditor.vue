<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { Eye, EyeOff, ShieldCheck } from 'lucide-vue-next'
import { errorMessage } from '../../../common/tauri'
import AppSelect from '../../../components/ui/AppSelect.vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import { connectionsApi } from '../api/connections'
import { type Connection, type ConnectionDraft, type Provider, providerNames } from '../model/connection'
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
const busy = ref<'test' | 'save' | null>(null)
const result = ref('')
const error = ref('')
const providerOptions = (Object.keys(providerNames) as Provider[]).map(value => ({ value, label: providerNames[value] }))
const addressingOptions = [
  { value: true, label: 'Path style' },
  { value: false, label: 'Virtual hosted' },
]
const updatesCredentials = computed(() => !props.profile || Boolean(accessKey.value || secretKey.value || sessionToken.value))
const valid = computed(() => Boolean(name.value.trim() && region.value.trim() && (endpoint.value.trim() || provider.value === 'aws') &&
  (!updatesCredentials.value || (accessKey.value.trim() && secretKey.value.trim()))))

watch(provider, value => { pathStyle.value = value !== 'aws' })
watch([name, provider, endpoint, region, pathStyle, known, accessKey, secretKey, sessionToken], () => { result.value = ''; error.value = '' })

function draft(): ConnectionDraft {
  return {
    id: props.profile?.id ?? null, name: name.value, provider: provider.value, endpoint: endpoint.value, region: region.value,
    pathStyle: pathStyle.value, knownBuckets: known.value.split(/[,\n]/).map(value => value.trim()).filter(Boolean),
    credentials: updatesCredentials.value ? { accessKey: accessKey.value.trim(), secretKey: secretKey.value, sessionToken: sessionToken.value || null } : null,
  }
}

async function run(action: 'test' | 'save'): Promise<void> {
  if (!valid.value || busy.value) return
  error.value = ''
  result.value = ''
  busy.value = action
  try {
    if (action === 'test') {
      const count = await connectionsApi.test(draft())
      result.value = `Connected. ${count} ${count === 1 ? 'bucket' : 'buckets'} available.`
      return
    }
    await connections.save(draft())
    emit('saved')
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    busy.value = null
  }
}
</script>

<template>
  <Modal :title="profile ? 'Edit connection' : 'New connection'" wide @close="!busy && emit('close')">
    <form class="modal-form" @submit.prevent="run('save')">
      <div class="form-grid">
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
      <p class="credential-note"><ShieldCheck :size="16" /> Saved credentials use your system credential store.</p>
      <Notice v-if="error" tone="error">{{ error }}</Notice>
      <Notice v-if="result" tone="success">{{ result }}</Notice>
      <div class="modal-actions">
        <button type="button" class="push-left" :disabled="!valid || !!busy" @click="run('test')">{{ busy === 'test' ? 'Testing…' : 'Test connection' }}</button>
        <button type="button" :disabled="!!busy" @click="emit('close')">Cancel</button>
        <button class="primary" :disabled="!valid || !!busy">{{ busy === 'save' ? 'Saving…' : 'Save connection' }}</button>
      </div>
    </form>
  </Modal>
</template>
