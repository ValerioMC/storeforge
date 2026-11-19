<script setup lang="ts">
import { onScopeDispose, ref, watch } from 'vue'
import { errorMessage } from '../../../common/tauri'
import Notice from '../../../components/ui/Notice.vue'
import type { Connection } from '../../connection/model/connection'
import { readDelegatedRgwUser } from '../api/rgw'
import type { RgwUser } from '../model/rgw'
import RgwAccessKeys from './RgwAccessKeys.vue'

const props = defineProps<{ connection: string }>()
const emit = defineEmits<{ open: [profile: Connection] }>()
const owner = ref<RgwUser>()
const loading = ref(false)
const error = ref('')
let generation = 0

async function reload(): Promise<void> {
  const request = ++generation
  loading.value = true
  error.value = ''
  try {
    const result = await readDelegatedRgwUser(props.connection)
    if (request === generation) owner.value = result
  } catch (failure) {
    if (request === generation) error.value = errorMessage(failure)
  } finally {
    if (request === generation) loading.value = false
  }
}

watch(() => props.connection, () => { owner.value = undefined; void reload() }, { immediate: true })
onScopeDispose(() => { generation++ })
</script>

<template>
  <div class="panel-form">
    <Notice v-if="error" tone="error">{{ error }} <button type="button" :disabled="loading" @click="reload">Retry</button></Notice>
    <p v-if="!owner && loading" role="status">Verifying the delegated RGW identity…</p>
    <RgwAccessKeys v-if="owner" :connection="connection" :owner="owner" scope="delegated" :loading="loading" :reload="reload" @open="emit('open', $event)" />
  </div>
</template>
