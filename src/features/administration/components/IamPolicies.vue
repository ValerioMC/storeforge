<script setup lang="ts">
import { onScopeDispose, ref } from 'vue'
import Notice from '../../../components/ui/Notice.vue'
import ConfirmDialog from '../../../components/ui/ConfirmDialog.vue'
import { errorMessage } from '../../../common/tauri'
import { listAttached, listInline, readInline, writeQuery } from '../api/iam'
import { useQueryPage } from '../composables/useQueryPage'
import { validPolicy, type IdentityKind } from '../model/iam'
import IamPicker from './IamPicker.vue'
import QueryPager from './QueryPager.vue'

const props = defineProps<{ connection: string; kind: IdentityKind | 'Role'; name: string; readOnly?: boolean }>()
const choices = ref<string[]>([])
const inline = ref<{ name: string; document: string }>()
const editing = ref(false)
const busy = ref(false)
const error = ref('')
const pending = ref<{ title: string; action: string; values: Record<string, string> }>()
const scope = () => [props.connection, props.kind, props.name]
const attached = useQueryPage(scope, marker => listAttached(props.connection, props.kind, props.name, marker), policy => policy.name)
const listed = useQueryPage(scope, marker => listInline(props.connection, props.kind, props.name, marker), name => name)
let disposed = false
onScopeDispose(() => { disposed = true })

async function inspect(name: string): Promise<void> {
  busy.value = true
  error.value = ''
  try {
    const document = await readInline(props.connection, props.kind, props.name, name)
    if (!disposed) {
      inline.value = { name, document }
      editing.value = true
    }
  } catch (failure) {
    if (!disposed) error.value = errorMessage(failure)
  } finally {
    busy.value = false
  }
}

async function mutate(writes: { action: string; values: Record<string, string> }[]): Promise<void> {
  if (props.readOnly || busy.value) return
  busy.value = true
  error.value = ''
  pending.value = undefined
  const failures: string[] = []
  try {
    for (const write of writes) {
      try {
        await writeQuery(props.connection, write.action, { [`${props.kind}Name`]: props.name, ...write.values })
        if (write.values.PolicyArn) choices.value = choices.value.filter(arn => arn !== write.values.PolicyArn)
        if (write.action.startsWith('Put')) editing.value = false
      } catch (failure) {
        failures.push(errorMessage(failure))
      }
    }
    if (!disposed) {
      error.value = failures.join('\n')
      await Promise.all([attached.reload(), listed.reload()])
    }
  } finally {
    busy.value = false
  }
}

function attach(): void {
  void mutate(choices.value.map(arn => ({ action: `Attach${props.kind}Policy`, values: { PolicyArn: arn } })))
}

function put(): void {
  if (!inline.value || !validPolicy(inline.value.document)) return
  void mutate([{ action: `Put${props.kind}Policy`, values: { PolicyName: inline.value.name, PolicyDocument: inline.value.document } }])
}

function remove(action: string, title: string, values: Record<string, string>): void {
  pending.value = { title, action, values }
}
</script>

<template>
  <div class="panel-form">
    <Notice v-if="error" tone="error">{{ error }}</Notice>
    <section class="panel-form">
      <h3>Attached policies</h3>
      <Notice v-if="attached.error.value" tone="error">{{ attached.error.value }}</Notice>
      <p v-else-if="attached.loading.value" role="status">Loading attached policies…</p>
      <p v-else-if="!attached.items.value.length" class="muted">No managed policies attached.</p>
      <div v-for="policy in attached.items.value" :key="policy.arn" class="settings-row">
        <div class="settings-description"><strong>{{ policy.name }}</strong><p>{{ policy.arn }}</p></div>
        <button v-if="!readOnly" type="button" :disabled="busy" @click="remove(`Detach${kind}Policy`, `Detach ${policy.name}?`, { PolicyArn: policy.arn })">Detach</button>
      </div>
      <QueryPager :busy="attached.loading.value" :previous="!!attached.markers.value.length" :next="!!attached.nextMarker.value" @previous="attached.previous" @next="attached.next" />
      <template v-if="!readOnly">
        <IamPicker v-model="choices" :connection="connection" kind="Policy" label="Managed policy picker" :disabled="busy" />
        <button type="button" :disabled="busy || !choices.length" @click="attach">Attach selected</button>
      </template>
    </section>
    <section class="panel-form">
      <div class="stage-heading">
        <h3>Inline policies</h3>
        <button v-if="!readOnly" type="button" :disabled="busy" @click="inline = { name: '', document: '' }; editing = true">New inline policy</button>
      </div>
      <Notice v-if="listed.error.value" tone="error">{{ listed.error.value }}</Notice>
      <p v-else-if="listed.loading.value" role="status">Loading inline policies…</p>
      <p v-else-if="!listed.items.value.length" class="muted">No inline policies.</p>
      <div v-for="policy in listed.items.value" :key="policy" class="settings-row">
        <div class="settings-description"><strong>{{ policy }}</strong></div>
        <button type="button" :disabled="busy" @click="inspect(policy)">{{ readOnly ? 'View JSON' : 'Edit JSON' }}</button>
        <button v-if="!readOnly" type="button" :disabled="busy" @click="remove(`Delete${kind}Policy`, `Delete ${policy}?`, { PolicyName: policy })">Delete</button>
      </div>
      <QueryPager :busy="listed.loading.value" :previous="!!listed.markers.value.length" :next="!!listed.nextMarker.value" @previous="listed.previous" @next="listed.next" />
      <form v-if="editing && inline" class="panel-form" @submit.prevent="put">
        <label>Policy name<input v-model="inline.name" :readonly="readOnly" required /></label>
        <label>Policy JSON<textarea v-model="inline.document" :readonly="readOnly" required rows="7" spellcheck="false" /></label>
        <div class="panel-actions">
          <button type="button" @click="editing = false">Close editor</button>
          <button v-if="!readOnly" class="primary" :disabled="busy || !inline.name.trim() || !validPolicy(inline.document)">Save policy</button>
        </div>
      </form>
    </section>
    <ConfirmDialog v-if="pending" :title="pending.title" :description="`This changes the permissions of ${name}.`"
      action="Apply change" danger @close="pending = undefined" @confirm="mutate([pending])" />
  </div>
</template>
