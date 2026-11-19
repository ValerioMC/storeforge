<script setup lang="ts">
import { onScopeDispose, ref } from 'vue'
import ConfirmDialog from '../../../components/ui/ConfirmDialog.vue'
import Notice from '../../../components/ui/Notice.vue'
import { errorMessage } from '../../../common/tauri'
import { listMembership, writeQuery } from '../api/iam'
import { useQueryPage } from '../composables/useQueryPage'
import { managedUser, type IdentityKind } from '../model/iam'
import IamPicker from './IamPicker.vue'
import QueryPager from './QueryPager.vue'

const props = defineProps<{ connection: string; kind: IdentityKind; name: string; readOnly: boolean }>()
const choices = ref<string[]>([])
const removing = ref('')
const error = ref('')
const busy = ref(false)
const { items, markers, nextMarker, loading, error: listError, next, previous, reload } = useQueryPage(
  () => [props.connection, props.kind, props.name], marker => listMembership(props.connection, props.kind, props.name, marker), item => item.name,
)
let disposed = false
onScopeDispose(() => { disposed = true })

async function change(action: 'AddUserToGroup' | 'RemoveUserFromGroup', candidates: string[]): Promise<void> {
  if (props.readOnly || busy.value) return
  busy.value = true
  removing.value = ''
  error.value = ''
  const failures: string[] = []
  try {
    for (const candidate of candidates) {
      const values = props.kind === 'User' ? { UserName: props.name, GroupName: candidate } : { UserName: candidate, GroupName: props.name }
      try {
        await writeQuery(props.connection, action, values)
        choices.value = choices.value.filter(value => value !== candidate)
      } catch (failure) {
        failures.push(errorMessage(failure))
      }
    }
    if (!disposed) {
      error.value = failures.join('\n')
      await reload()
    }
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <section class="panel-form">
    <h3>{{ kind === 'User' ? 'Group memberships' : 'Group members' }}</h3>
    <Notice v-if="error || listError" tone="error">{{ error || listError }}</Notice>
    <p v-else-if="loading" role="status">Loading memberships…</p>
    <p v-else-if="!items.length" class="muted">No memberships.</p>
    <div v-for="member in items" :key="member.id" class="settings-row">
      <div class="settings-description"><strong>{{ member.name }}</strong><p>{{ member.path }}</p></div>
      <button v-if="!readOnly && !(kind === 'Group' && managedUser(member.name))" :disabled="busy" @click="removing = member.name">Remove membership</button>
    </div>
    <QueryPager :busy="loading" :previous="!!markers.length" :next="!!nextMarker" @previous="previous" @next="next" />
    <IamPicker v-if="!readOnly" v-model="choices" :connection="connection" :kind="kind === 'User' ? 'Group' : 'User'" label="Membership picker" :disabled="busy" />
    <button v-if="!readOnly" type="button" :disabled="busy || !choices.length" @click="change('AddUserToGroup', choices)">Add selected</button>
    <ConfirmDialog v-if="removing" title="Remove membership?" :description="`Remove the membership between ${name} and ${removing}.`"
      action="Remove membership" danger @close="removing = ''" @confirm="change('RemoveUserFromGroup', [removing])" />
  </section>
</template>
