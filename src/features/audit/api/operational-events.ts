import { desktop } from '../../../common/tauri'
import type {
  BucketUsageSummary, CostMonth, EndpointHealth, HistoryCollectionResult, HistoryFilter, HistoryScope,
  InboxCount, NotificationInbox, QuotaPreferences, ServiceAuditPage, WebhookDelivery, WebhookEndpoint,
} from '../model/operational-events'

export const operationsApi = {
  history: (id: string, scope: HistoryScope, filter: HistoryFilter) =>
    desktop<ServiceAuditPage>('operational_events', { id, scope, filter }),
  inbox: (id: string) => desktop<NotificationInbox>('operational_inbox', { id }),
  read: (id: string, change: { notification_ids?: number[]; all?: boolean }) =>
    desktop<InboxCount>('read_operational_notifications', { id, change }),
  quotaPreferences: (id: string, draft?: QuotaPreferences) =>
    desktop<QuotaPreferences>('operational_quota_preferences', { id, draft }),
  testSmtp: (id: string) => desktop<{ recipient: string }>('test_operational_smtp', { id }),
  webhooks: (id: string) => desktop<WebhookEndpoint[]>('operational_webhooks', { id }),
  deliveries: (id: string, endpoint: number) => desktop<WebhookDelivery[]>('operational_webhook_deliveries', { id, endpoint }),
  health: (id: string) => desktop<{ endpoints: EndpointHealth[] }>('operational_health', { id }),
  historySummary: (id: string) => desktop<{ summary: { latest_collected_at: string | null } }>('operational_usage_history', { id }),
  usage: (id: string, endpoint: number) => desktop<{ aggregate: BucketUsageSummary }>('operational_bucket_usage', { id, endpoint }),
  cost: (id: string) => desktop<{ history: CostMonth[] }>('operational_cost_history', { id }),
  collectHistory: (id: string) => desktop<HistoryCollectionResult>('collect_operational_usage_history', { id }),
}
