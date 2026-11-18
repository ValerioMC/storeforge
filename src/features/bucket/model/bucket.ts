export interface Bucket {
  name: string
  createdAt: string | null
}

export type Configuration = 'versioning' | 'lifecycle' | 'cors' | 'policy' | 'acl' | 'ownershipControls' | 'publicAccessBlock' | 'encryption' | 'objectLock' | 'tagging' | 'logging' | 'notification' | 'replication' | 'website' | 'requestPayment'

export type ConfigurationChange = { action: 'read' | 'remove' } | { action: 'write'; document: string }
