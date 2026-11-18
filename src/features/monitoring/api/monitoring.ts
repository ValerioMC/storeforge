import { desktop } from '../../../common/tauri'
import type { CheckMode, MonitoringView } from '../model/monitoring'

export function readMonitoring(connection: string): Promise<MonitoringView> {
  return desktop('monitoring_cached', { connection })
}

export function collectMonitoring(connection: string): Promise<MonitoringView> {
  return desktop('monitoring_collect', { connection })
}

export function checkEndpoint(connection: string, mode: CheckMode): Promise<MonitoringView> {
  return desktop('monitoring_check', { connection, mode })
}
