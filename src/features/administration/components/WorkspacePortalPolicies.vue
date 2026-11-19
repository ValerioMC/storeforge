<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import AppSelect from '../../../components/ui/AppSelect.vue'
import Notice from '../../../components/ui/Notice.vue'
import { errorMessage } from '../../../common/tauri'
import { workspaceSettingsApi } from '../api/workspace-settings'
import { overrideOptions, overrideValue, portalFeatures, type AccountPolicy, type AccountPortalSettings, type OverrideValue, type PolicyAccounts } from '../model/workspace-settings'
import QueryPager from './QueryPager.vue'

const props = defineProps<{ serviceId: string }>()
const accounts = ref<PolicyAccounts>()
const account = ref('')
const page = ref(1)
const search = ref('')
const appliedSearch = ref('')
const settings = ref<AccountPortalSettings>()
const pending = ref(false)
const error = ref('')
const options = computed(() => accounts.value?.items.map(item => ({ value: String(item.id), label: item.name })) ?? [])

onMounted(() => loadAccounts(1))

function submitSearch(): void {
  appliedSearch.value = search.value.trim()
  void loadAccounts(1)
}

async function loadAccounts(value: number): Promise<void> {
  pending.value = true
  settings.value = undefined
  account.value = ''
  error.value = ''
  try {
    accounts.value = await workspaceSettingsApi.accounts(props.serviceId, value, appliedSearch.value)
    page.value = value
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    pending.value = false
  }
}

async function setAccount(id: string): Promise<void> {
  account.value = id
  settings.value = undefined
  await loadSettings()
}

async function loadSettings(change?: { policy: AccountPolicy; value: boolean | null }): Promise<void> {
  pending.value = true
  error.value = ''
  try {
    settings.value = await workspaceSettingsApi.portalAccount(props.serviceId, Number(account.value), change)
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    pending.value = false
  }
}

function changePolicy(policy: AccountPolicy, value: OverrideValue): void {
  void loadSettings({ policy, value: value === 'inherit' ? null : value === 'allow' })
}
</script>

<template>
  <section class="settings-sheet">
    <div class="settings-row"><h2>Portal account overrides</h2></div>
    <Notice v-if="error" tone="error">{{ error }}</Notice>
    <div class="settings-row">
      <form class="settings-search" @submit.prevent="submitSearch">
        <input v-model="search" type="search" aria-label="Search accounts" placeholder="Search accounts" />
        <button type="submit" :disabled="pending">Search</button>
      </form>
      <AppSelect :model-value="account" :options="options" placeholder="Select account" aria-label="Portal account"
        :disabled="pending" @update:model-value="setAccount" />
      <QueryPager v-if="accounts" :busy="pending" :previous="page > 1" :next="accounts.has_next"
        @previous="loadAccounts(page - 1)" @next="loadAccounts(page + 1)" />
    </div>
    <div v-if="pending" class="settings-row" role="status">Reading account policies…</div>
    <div v-for="(label, policy) in settings ? portalFeatures : {}" :key="policy" class="settings-row">
      <div class="settings-description">
        <strong>{{ label }}</strong>
        <p>Effective: {{ settings?.effective[policy] ? 'Enabled' : 'Disabled' }}</p>
      </div>
      <AppSelect :model-value="overrideValue(settings?.admin_override[policy])" :options="overrideOptions"
        :aria-label="`${label} override`" :disabled="pending" @update:model-value="changePolicy(policy, $event)" />
    </div>
  </section>
</template>

<style scoped>
h2 { font-size: 15px; margin: 0; }
.settings-search { display: flex; flex-wrap: wrap; gap: 6px; }
.settings-row { flex-wrap: wrap; }
</style>
