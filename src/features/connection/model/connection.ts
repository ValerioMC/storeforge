export type Provider = 'aws' | 'ceph' | 'compatible'

export interface Connection {
  id: string
  name: string
  provider: Provider
  endpoint: string
  region: string
  pathStyle: boolean
  knownBuckets: string[]
}

export interface Credentials {
  accessKey: string
  secretKey: string
  sessionToken: string | null
}

export interface ConnectionDraft extends Omit<Connection, 'id'> {
  id: string | null
  credentials: Credentials | null
}

export const providerNames: Record<Provider, string> = {
  aws: 'Amazon S3',
  ceph: 'Ceph RGW',
  compatible: 'S3 compatible',
}
