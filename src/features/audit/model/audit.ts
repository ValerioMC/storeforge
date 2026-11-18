export interface AuditEvent {
  id: string
  timestamp: string
  connection: string
  action: string
  resource: string | null
  success: boolean
}
