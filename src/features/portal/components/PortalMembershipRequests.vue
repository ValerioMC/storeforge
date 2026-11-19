<script setup lang="ts">
import { onMounted } from 'vue'
import Notice from '../../../components/ui/Notice.vue'
import { sharingApi } from '../api/sharing'
import { usePortalTask } from '../composables/usePortalTask'
import type { MembershipReceipt } from '../model/sharing'
import type { PortalScope } from '../model/spaces'

const props = defineProps<{ scope: PortalScope }>()
const { data, pending, error, run } = usePortalTask<MembershipReceipt[]>()

onMounted(() => void run(() => sharingApi.requests(props.scope)))
</script>

<template>
  <section class="portal-panel">
    <h2>Project requests</h2>
    <Notice v-if="error" tone="error">{{ error }}</Notice>
    <p v-if="pending" role="status" class="muted portal-note">Loading requests…</p>
    <ul v-if="data?.length" class="portal-list">
      <li v-for="request in data" :key="request.id" class="portal-between">
        <strong>Request #{{ request.id }}</strong>
        <span class="chip">{{ request.status }}</span>
        <span v-if="request.error_message" class="muted">{{ request.error_message }}</span>
      </li>
    </ul>
    <p v-else-if="data" class="muted portal-note">No project requests.</p>
  </section>
</template>
