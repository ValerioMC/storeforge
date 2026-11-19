import { open, save } from '@tauri-apps/plugin-dialog'
import { desktop, isTauri } from '../../../common/tauri'
import type { Location, ObjectVersion } from '../../browser/model/objects'
import type { Comparison, Integrity, Job, Transfer, Usage } from '../model/transfer'

import type { BucketComparison, BucketComparisonRequest, BucketPair, ComparisonScope, Remediation } from '../model/bucket-comparison'
import type { TransferAccess } from '../model/access'
import type { VersionPlanItem, VersionRule } from '../model/batch'

export const transfersApi = {
  jobs: () => isTauri() ? desktop<Job[]>('list_jobs') : Promise.resolve([]),
  start: (transfer: Transfer, access?: TransferAccess) => desktop<string>('start_transfer', { transfer, access }),
  cancel: (id: string) => desktop<void>('cancel_job', { id }),
  compare: (source: Location, target: Location) => desktop<Comparison>('compare_locations', { source, target }),
  compareBuckets: (source: string, target: string, request: BucketComparisonRequest) =>
    desktop<BucketComparison>('compare_buckets', { source, target, request }),
  remediate: (scope: ComparisonScope, pair: BucketPair, action: Remediation) =>
    desktop<void>('remediate_bucket_difference', { scope, pair, action }),
  usage: (location: Location, includeVersions: boolean) => desktop<Usage>('location_usage', { location, includeVersions }),
  integrity: (location: Location) => desktop<Integrity>('check_integrity', { location }),
  history: (location: Location) => desktop<ObjectVersion[]>('prefix_history', { location }),
  versionPlan: (location: Location, rule: VersionRule) => desktop<VersionPlanItem[]>('preview_version_plan', { location, rule }),
  async exportReport(name: string, report: string): Promise<void> {
    const path = await save({ defaultPath: name, title: 'Save operation report' })
    if (path) await desktop<void>('export_transfer_report', { path, report })
  },
  chooseFiles: () => open({ multiple: true, title: 'Choose files to upload' }),
  chooseDirectory: () => open({ directory: true, title: 'Choose download folder' }),
  saveFile: (name: string) => save({ defaultPath: name, title: 'Save downloaded file' }),
}
