export type EventStatus = 'success' | 'failure' | 'partial'
export type HistoryScope = 'admin' | 'manager'

export interface HistoryFilter {
  cursor?: number
  context_id?: string
  search?: string
  actor?: string
  status?: EventStatus
}

export interface ServiceAuditEvent {
  id: number
  created_at: string
  user_email: string
  action: string
  entity_type: string | null
  entity_id: string | null
  context_name: string | null
  status: EventStatus
  user_role: string | null
  scope: string | null
  message: string | null
  metadata: Record<string, unknown> | null
}

export interface ServiceAuditPage {
  logs: ServiceAuditEvent[]
  next_cursor: number | null
}

export interface ServiceNotification {
  id: number
  type: string
  severity: 'info' | 'warning' | 'error'
  title: string
  message: string
  created_at: string
  read_at: string | null
}

export interface InboxCount { unread_count: number }
export interface NotificationInbox extends InboxCount { items: ServiceNotification[] }

export interface QuotaPreferences {
  quota_alerts_enabled: boolean
  quota_alerts_global_watch: boolean
}

export interface WebhookDelivery {
  id: number
  event_type: string
  is_test: boolean
  status: 'pending' | 'retrying' | 'delivered' | 'failed'
  attempt_count: number
  last_http_status: number | null
  last_error: string | null
  updated_at: string
  delivered_at: string | null
}

export interface WebhookEndpoint {
  id: number
  name: string
  url: string
  enabled: boolean
  event_types: string[]
  has_signing_secret: boolean
  last_delivery: WebhookDelivery | null
}

export interface EndpointHealth {
  name: string
  status: string
  checked_at: string | null
  error_message: string | null
}

export interface BucketUsageSummary {
  newest_snapshot_at: string | null
  warnings: string[]
  missing_bucket_count: number
  partial_scan_count: number
}

export interface CostMonth {
  month: string
  coverage: { days_collected: number; days_in_month: number }
}

export interface HistoryCollectionResult {
  subjects_processed: number
  status: 'skipped' | null
  reason: string | null
  errors: { subject_type: string; subject_id: number; error: string }[]
  warnings: (string | { subject_type: string; subject_id: number; warning: string })[]
}

export function eventLabel(value: string): string {
  return value.replace(/[_.]/g, ' ').replace(/^./, letter => letter.toUpperCase())
}
