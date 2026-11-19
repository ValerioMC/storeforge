<script setup lang="ts">
import { computed, ref } from 'vue'
import Modal from '../../../components/ui/Modal.vue'
import Notice from '../../../components/ui/Notice.vue'
import { validPolicy, type IdentityKind, type InlinePolicy, type PrivateAccessDraft } from '../model/iam'
import IamPicker from './IamPicker.vue'

const props = defineProps<{ connection: string; kind: IdentityKind; privateAccess?: boolean; busy: boolean; error: string }>()
const emit = defineEmits<{ close: []; save: [draft: PrivateAccessDraft, path: string] }>()
const name = ref('')
const path = ref('/')
const browser = ref(true)
const manager = ref(true)
const members = ref<string[]>([])
const policies = ref<string[]>([])
const inlinePolicies = ref<InlinePolicy[]>([])
const allowed = computed(() => !!name.value.trim() && (
  props.privateAccess
    ? (browser.value || manager.value) && inlinePolicies.value.every(policy => policy.name.trim() && validPolicy(policy.document))
    : !!path.value.trim()
))

function save(): void {
  if (!allowed.value || props.busy) return
  emit('save', {
    name: name.value.trim(), browser: browser.value, manager: manager.value,
    groups: members.value, policies: policies.value, inlinePolicies: inlinePolicies.value,
  }, path.value.trim())
}
</script>

<template>
  <Modal :title="privateAccess ? 'Provision private connection' : `Create IAM ${kind.toLowerCase()}`" wide @close="!busy && emit('close')">
    <form class="modal-form" @submit.prevent="save">
      <Notice v-if="error" tone="error">{{ error }}</Notice>
      <label>{{ privateAccess ? 'Connection name' : `${kind} name` }}
        <input v-model="name" required :disabled="busy" autocomplete="off" />
      </label>
      <label v-if="!privateAccess">IAM path<input v-model="path" required :disabled="busy" autocomplete="off" /></label>
      <template v-else>
        <p class="muted">Storeforge creates a dedicated IAM user and saves its key in this device’s credential store.</p>
        <div class="panel-actions">
          <label class="checkbox-label"><input v-model="browser" type="checkbox" :disabled="busy" /> Browser</label>
          <label class="checkbox-label"><input v-model="manager" type="checkbox" :disabled="busy" /> Manager</label>
        </div>
      </template>
      <IamPicker v-model="members" :connection="connection" :kind="kind === 'User' ? 'Group' : 'User'" :label="kind === 'User' ? 'Groups' : 'Members'" :disabled="busy" />
      <IamPicker v-model="policies" :connection="connection" kind="Policy" label="Managed policies" :disabled="busy" />
      <section v-if="privateAccess" class="panel-form">
        <div class="stage-heading"><h3>Inline policies</h3><button type="button" :disabled="busy" @click="inlinePolicies.push({ name: '', document: '' })">Add inline policy</button></div>
        <div v-for="(policy, index) in inlinePolicies" :key="index" class="panel-form">
          <label>Policy name<input v-model="policy.name" required :disabled="busy" /></label>
          <label>Policy JSON<textarea v-model="policy.document" required rows="5" :disabled="busy" spellcheck="false" /></label>
          <button type="button" :disabled="busy" @click="inlinePolicies.splice(index, 1)">Remove inline policy</button>
        </div>
      </section>
      <div class="modal-actions">
        <button type="button" :disabled="busy" @click="emit('close')">Cancel</button>
        <button class="primary" :disabled="busy || !allowed">{{ busy ? 'Applying…' : privateAccess ? 'Provision connection' : `Create ${kind.toLowerCase()}` }}</button>
      </div>
    </form>
  </Modal>
</template>
