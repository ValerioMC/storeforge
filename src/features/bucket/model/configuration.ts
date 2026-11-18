import type { Configuration } from './bucket'

interface Option {
  id: Configuration
  label: string
  format: 'XML' | 'JSON'
  remove: boolean
  example: string
}

function xml(element: string, body: string): string {
  return `<${element} xmlns="http://s3.amazonaws.com/doc/2006-03-01/">\n${body}\n</${element}>`
}

export const configurations: readonly Option[] = [
  { id: 'versioning', label: 'Versioning', format: 'XML', remove: false, example: xml('VersioningConfiguration', '  <Status>Enabled</Status>') },
  { id: 'lifecycle', label: 'Lifecycle rules', format: 'XML', remove: true, example: xml('LifecycleConfiguration', '  <Rule>\n    <ID>Expire old logs</ID>\n    <Filter><Prefix>logs/</Prefix></Filter>\n    <Status>Disabled</Status>\n    <Expiration><Days>365</Days></Expiration>\n  </Rule>') },
  { id: 'cors', label: 'CORS', format: 'XML', remove: true, example: xml('CORSConfiguration', '  <CORSRule>\n    <AllowedOrigin>https://example.com</AllowedOrigin>\n    <AllowedMethod>GET</AllowedMethod>\n  </CORSRule>') },
  { id: 'policy', label: 'Bucket policy', format: 'JSON', remove: true, example: '{\n  "Version": "2012-10-17",\n  "Statement": []\n}' },
  { id: 'acl', label: 'Access control list', format: 'XML', remove: false, example: '' },
  { id: 'ownershipControls', label: 'Object ownership', format: 'XML', remove: true, example: xml('OwnershipControls', '  <Rule><ObjectOwnership>BucketOwnerEnforced</ObjectOwnership></Rule>') },
  { id: 'publicAccessBlock', label: 'Block public access', format: 'XML', remove: true, example: xml('PublicAccessBlockConfiguration', '  <BlockPublicAcls>true</BlockPublicAcls>\n  <IgnorePublicAcls>true</IgnorePublicAcls>\n  <BlockPublicPolicy>true</BlockPublicPolicy>\n  <RestrictPublicBuckets>true</RestrictPublicBuckets>') },
  { id: 'encryption', label: 'Default encryption', format: 'XML', remove: true, example: xml('ServerSideEncryptionConfiguration', '  <Rule>\n    <ApplyServerSideEncryptionByDefault><SSEAlgorithm>AES256</SSEAlgorithm></ApplyServerSideEncryptionByDefault>\n  </Rule>') },
  { id: 'objectLock', label: 'Object Lock', format: 'XML', remove: false, example: xml('ObjectLockConfiguration', '  <ObjectLockEnabled>Enabled</ObjectLockEnabled>') },
  { id: 'tagging', label: 'Bucket tags', format: 'XML', remove: true, example: xml('Tagging', '  <TagSet><Tag><Key>environment</Key><Value>production</Value></Tag></TagSet>') },
  { id: 'logging', label: 'Access logging', format: 'XML', remove: false, example: xml('BucketLoggingStatus', '') },
  { id: 'notification', label: 'Notifications', format: 'XML', remove: false, example: xml('NotificationConfiguration', '') },
  { id: 'replication', label: 'Replication', format: 'XML', remove: true, example: '' },
  { id: 'website', label: 'Static website', format: 'XML', remove: true, example: xml('WebsiteConfiguration', '  <IndexDocument><Suffix>index.html</Suffix></IndexDocument>') },
  { id: 'requestPayment', label: 'Request payment', format: 'XML', remove: false, example: xml('RequestPaymentConfiguration', '  <Payer>BucketOwner</Payer>') },
]
