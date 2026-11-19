<script setup lang="ts">
import { onMounted, ref } from 'vue'
import Notice from '../../../components/ui/Notice.vue'
import { errorMessage } from '../../../common/tauri'
import { workspaceSettingsApi } from '../api/workspace-settings'
import { settingLabel, type Readiness } from '../model/workspace-settings'

const props = defineProps<{ serviceId: string }>()
const report = ref<Readiness>()
const error = ref('')
const labels = { blocked: 'Blocked', critical: 'Critical', warning: 'Warning', manual: 'Manual check', ok: 'Passed' }

onMounted(load)

async function load(): Promise<void> {
  try {
    report.value = await workspaceSettingsApi.readiness(props.serviceId)
  } catch (failure) {
    error.value = errorMessage(failure)
  }
}

function documentationUrl(value: string): string | undefined {
  try {
    const url = new URL(value)
    return url.protocol === 'https:' || url.protocol === 'http:' ? url.href : undefined
  } catch {
    return undefined
  }
}
</script>

<template>
  <Notice v-if="error" tone="error">{{ error }}</Notice>
  <section v-else-if="report" class="settings-sheet">
    <div class="settings-row">
      <div class="settings-description">
        <strong>{{ settingLabel(report.status) }}</strong>
        <p>{{ settingLabel(report.environment) }} · {{ settingLabel(report.profile) }} deployment</p>
      </div>
    </div>
    <div v-for="finding in report.findings" :key="finding.code" class="settings-row">
      <div class="settings-description">
        <strong>{{ finding.label }}</strong>
        <p>{{ finding.message }}</p>
      </div>
      <span class="chip">{{ labels[finding.level] }}</span>
      <span v-if="finding.blocks_startup" class="chip">Blocks startup</span>
      <a v-if="documentationUrl(finding.documentation_url)" :href="documentationUrl(finding.documentation_url)"
        target="_blank" rel="noopener noreferrer">Remediation</a>
    </div>
  </section>
  <p v-else role="status">Reading readiness findings…</p>
</template>
