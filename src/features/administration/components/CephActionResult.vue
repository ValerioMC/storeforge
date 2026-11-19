<script setup lang="ts">
import Notice from '../../../components/ui/Notice.vue'
import type { CephOutcome } from '../model/ceph-operations'

defineProps<{ result: CephOutcome }>()
</script>

<template>
  <Notice :tone="result.success ? 'info' : 'error'">
    {{ result.message }}
    <span v-if="result.status !== null"> HTTP {{ result.status }}.</span>
    <span v-if="result.code"> Ceph error {{ result.code }}.</span>
  </Notice>
  <details v-if="result.details">
    <summary>Provider diagnostics</summary>
    <pre class="ceph-diagnostics">{{ JSON.stringify(result.details, null, 2) }}</pre>
  </details>
</template>

<style scoped>
.ceph-diagnostics { max-height: 260px; overflow: auto; white-space: pre-wrap; }
</style>
