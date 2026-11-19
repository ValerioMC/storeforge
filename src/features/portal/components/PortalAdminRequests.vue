<script setup lang="ts">
import { computed, onMounted } from 'vue'
import WorkspaceHeader from '../../../components/shell/WorkspaceHeader.vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import Notice from '../../../components/ui/Notice.vue'
import { useSharedProfile } from '../../connection/composables/useSharedProfile'
import PortalRequests from './PortalRequests.vue'

const emit = defineEmits<{ connections: [] }>()
const { serviceId, options, pending, error, profile, loadProfiles } = useSharedProfile(status => !!status.session?.access.admin.available)
const admin = computed(() => !!profile.value?.session?.access.admin.available)

onMounted(loadProfiles)
</script>

<template>
  <section class="workspace-page portal-page">
    <WorkspaceHeader title="Portal requests" description="Review requests submitted by Portal projects.">
      <template #actions><button type="button" @click="emit('connections')">Service connections</button></template>
    </WorkspaceHeader>
    <Notice v-if="error" tone="error">{{ error }}</Notice>
    <label v-if="options.length" class="portal-inline">Shared service <AppSelect v-model="serviceId" :options="options" aria-label="Admin request service" /></label>
    <p v-if="pending" class="muted portal-note" role="status">Loading services…</p>
    <PortalRequests v-else-if="admin" :key="serviceId" :id="serviceId" />
    <Notice v-else>Authorize a shared service with an Admin session to review requests.</Notice>
  </section>
</template>
