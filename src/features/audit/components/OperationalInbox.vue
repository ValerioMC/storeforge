<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import Notice from '../../../components/ui/Notice.vue'
import { formatDate } from '../../../common/format'
import { errorMessage } from '../../../common/tauri'
import { operationsApi } from '../api/operational-events'
import { eventLabel, type NotificationInbox, type QuotaPreferences } from '../model/operational-events'

const props = defineProps<{ serviceId: string; inbox?: NotificationInbox; admin: boolean; superadmin: boolean }>()
const emit = defineEmits<{ refresh: [] }>()
const preferences = ref<QuotaPreferences>()
const pending = ref(false)
const error = ref('')
const receipt = ref('')
const hasUnread = computed(() => !!props.inbox?.unread_count)

onMounted(loadPreferences)

async function loadPreferences(): Promise<void> {
  try {
    preferences.value = await operationsApi.quotaPreferences(props.serviceId)
  } catch (failure) {
    error.value = errorMessage(failure)
  }
}

async function markRead(id?: number): Promise<void> {
  await perform(async () => {
    await operationsApi.read(props.serviceId, id == null ? { all: true } : { notification_ids: [id] })
    emit('refresh')
  })
}

async function savePreferences(): Promise<void> {
  if (!preferences.value) return
  const draft = { ...preferences.value }
  await perform(async () => {
    preferences.value = await operationsApi.quotaPreferences(props.serviceId, draft)
    receipt.value = 'Quota alert preferences saved.'
  })
}

async function testSmtp(): Promise<void> {
  await perform(async () => {
    const result = await operationsApi.testSmtp(props.serviceId)
    receipt.value = `Test email sent to ${result.recipient}.`
  })
}

async function perform(operation: () => Promise<void>): Promise<void> {
  pending.value = true
  error.value = ''
  receipt.value = ''
  try {
    await operation()
  } catch (failure) {
    error.value = errorMessage(failure)
  } finally {
    pending.value = false
  }
}
</script>

<template>
  <Notice v-if="error" tone="error">{{ error }}</Notice>
  <Notice v-if="receipt" tone="success">{{ receipt }}</Notice>
  <section class="settings-sheet">
    <div class="settings-row">
      <h2>Notifications</h2>
      <div class="inbox-actions">
        <button type="button" :disabled="pending" @click="emit('refresh')">Refresh inbox</button>
        <button type="button" :disabled="pending || !hasUnread" @click="markRead()">Mark all read</button>
      </div>
    </div>
    <p v-if="inbox && !inbox.items.length" class="settings-row">No notifications.</p>
    <div v-for="notification in inbox?.items" :key="notification.id" class="settings-row">
      <div class="settings-description">
        <strong>{{ notification.title }}</strong>
        <p>{{ notification.message }}</p>
        <small>{{ formatDate(notification.created_at, true) }} · {{ eventLabel(notification.type) }}</small>
      </div>
      <span class="chip">{{ eventLabel(notification.severity) }}</span>
      <button v-if="!notification.read_at" type="button" :disabled="pending" :aria-label="`Mark ${notification.title} read`" @click="markRead(notification.id)">Mark read</button>
      <span v-else class="muted">Read</span>
    </div>
  </section>
  <form v-if="preferences" class="settings-sheet" @submit.prevent="savePreferences">
    <div class="settings-row"><h2>Quota alerts</h2></div>
    <label class="settings-row">Quota email alerts <input v-model="preferences.quota_alerts_enabled" type="checkbox" :disabled="pending" /></label>
    <label v-if="admin" class="settings-row">Watch quotas across all managed resources <input v-model="preferences.quota_alerts_global_watch" type="checkbox" :disabled="pending" /></label>
    <div class="settings-row inbox-actions">
      <button type="submit" :disabled="pending">Save email preferences</button>
      <button v-if="superadmin" type="button" :disabled="pending" @click="testSmtp">Send SMTP test to my address</button>
    </div>
  </form>
</template>

<style scoped>
h2 { font-size: 15px; margin: 0; }
.settings-row { flex-wrap: wrap; }
.inbox-actions { display: flex; flex-wrap: wrap; gap: 6px; }
</style>
