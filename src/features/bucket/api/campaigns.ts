import { desktop } from '../../../common/tauri'
import type { BucketUsage, Reading } from '../../monitoring/model/monitoring'
import type { BucketQuota } from '../model/bucket'
import type { CampaignGateway } from '../model/campaign'
import { scopedBucketConfiguration } from './workbench'

export const campaignGateway: CampaignGateway = {
  read: (row, section) => scopedBucketConfiguration(row.target, section, { action: 'read' }),
  quota: row => desktop<BucketQuota>('bucket_campaign_quota', { target: row.target }),
  configure: (row, update) => desktop<string>('apply_bucket_campaign_configuration', { target: row.target, update }),
  updateQuota: (row, expected, draft) => desktop<BucketQuota>('apply_bucket_campaign_quota', { target: row.target, update: { expected, draft } }),
  usage: row => desktop<Reading<BucketUsage>>('recalculate_bucket_campaign_usage', { target: row.target }),
}
