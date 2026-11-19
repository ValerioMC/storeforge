import { desktop } from '../../../common/tauri'
import type { CostCollection, CostReport, CostSettings, ScopedCostSummary } from '../model/storage-costs'

export function readCostReport(month: string): Promise<CostReport> {
  return desktop('storage_cost_report', { month: `${month}-01` })
}

export function saveCostSettings(settings: CostSettings): Promise<void> {
  return desktop('storage_cost_settings', { settings })
}

export function collectDailyCosts(day: string, endpoint: string): Promise<CostCollection> {
  return desktop('storage_cost_collect', { day, endpoint: endpoint || null })
}

export function readScopedCosts(connection: string, month: string): Promise<ScopedCostSummary | null> {
  return desktop('storage_cost_summary', { connection, month: `${month}-01` })
}

export function exportCosts(month: string, csv: string): Promise<boolean> {
  return desktop('storage_cost_export', { month: `${month}-01`, csv })
}
