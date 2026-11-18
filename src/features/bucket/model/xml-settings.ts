import type { Configuration } from './bucket'

interface Field {
  name: string
  label: string
  multiple?: boolean
}

export interface XmlValueField extends Field {
  kind: 'text' | 'number' | 'check' | 'select'
  options?: readonly string[]
  empty?: string
  attribute?: boolean
  allowEmpty?: boolean
}

interface GroupField extends Field {
  kind: 'group'
  fields: readonly XmlField[]
}

export type XmlField = XmlValueField | GroupField

export interface XmlSettings {
  root: string
  fields: readonly XmlField[]
}

function text(name: string, label: string, multiple = false): XmlValueField {
  return { name, label, kind: 'text', multiple }
}

function number(name: string, label: string): XmlValueField {
  return { name, label, kind: 'number' }
}

function check(name: string, label: string): XmlValueField {
  return { name, label, kind: 'check' }
}

function select(name: string, label: string, options: readonly string[], empty?: string): XmlValueField {
  return { name, label, kind: 'select', options, empty }
}

function group(name: string, label: string, fields: readonly XmlField[], multiple = false): GroupField {
  return { name, label, kind: 'group', fields, multiple }
}

const status = ['Enabled', 'Disabled']
const storageClass = text('StorageClass', 'Storage class')
const tag = (multiple = false) => group('Tag', 'Tag', [
  text('Key', 'Key'),
  { ...text('Value', 'Value'), allowEmpty: true },
], multiple)
const prefix = text('Prefix', 'Key prefix')
const sizeFilters = [
  number('ObjectSizeGreaterThan', 'Minimum size in bytes (exclusive)'),
  number('ObjectSizeLessThan', 'Maximum size in bytes (exclusive)'),
]
const encryptionKey = text('KMSMasterKeyID', 'KMS key ID or ARN')
const websiteRedirect = [
  text('HostName', 'Host name'),
  select('Protocol', 'Protocol', ['http', 'https']),
]
const grantPermission = select('Permission', 'Permission', ['FULL_CONTROL', 'READ', 'WRITE', 'READ_ACP', 'WRITE_ACP'])
const principalFields = [
  { ...select('xsi:type', 'Grantee type', ['CanonicalUser', 'Group', 'AmazonCustomerByEmail']), attribute: true },
  text('ID', 'Canonical user ID'),
  text('URI', 'Group URI'),
  text('EmailAddress', 'Email address'),
]
const notificationFields = [
  text('Id', 'Notification ID'),
  text('Event', 'Event', true),
  group('Filter', 'Object key filter', [group('S3Key', 'Key rules', [
    group('FilterRule', 'Filter rule', [
      select('Name', 'Match', ['prefix', 'suffix']),
      text('Value', 'Value'),
    ], true),
  ])]),
]

export const xmlSettings: Record<Exclude<Configuration, 'policy'>, XmlSettings> = {
  versioning: {
    root: 'VersioningConfiguration',
    fields: [
      select('Status', 'Versioning status', ['Enabled', 'Suspended'], 'Unversioned'),
      select('MfaDelete', 'MFA delete', status),
    ],
  },
  lifecycle: {
    root: 'LifecycleConfiguration',
    fields: [group('Rule', 'Lifecycle rule', [
      text('ID', 'Rule ID'),
      select('Status', 'Status', status),
      group('Filter', 'Filter', [prefix, tag(), ...sizeFilters, group('And', 'Combined filters', [prefix, tag(true), ...sizeFilters])]),
      text('Prefix', 'Legacy key prefix'),
      group('Expiration', 'Expiration', [
        number('Days', 'Expire after days'),
        text('Date', 'Expiration date (ISO 8601)'),
        check('ExpiredObjectDeleteMarker', 'Remove expired object delete markers'),
      ]),
      group('Transition', 'Transition', [number('Days', 'After days'), text('Date', 'Date (ISO 8601)'), storageClass], true),
      group('NoncurrentVersionExpiration', 'Noncurrent expiration', [
        number('NoncurrentDays', 'Expire after days'),
        number('NewerNoncurrentVersions', 'Newer versions to retain'),
      ]),
      group('NoncurrentVersionTransition', 'Noncurrent transition', [
        number('NoncurrentDays', 'After days'),
        number('NewerNoncurrentVersions', 'Newer versions to retain'),
        storageClass,
      ], true),
      group('AbortIncompleteMultipartUpload', 'Abort incomplete multipart uploads', [
        number('DaysAfterInitiation', 'After days'),
      ]),
    ], true)],
  },
  cors: {
    root: 'CORSConfiguration',
    fields: [group('CORSRule', 'CORS rule', [
      text('ID', 'Rule ID'),
      text('AllowedOrigin', 'Allowed origin', true),
      { ...select('AllowedMethod', 'Allowed method', ['GET', 'PUT', 'POST', 'DELETE', 'HEAD']), multiple: true },
      text('AllowedHeader', 'Allowed header', true),
      text('ExposeHeader', 'Exposed header', true),
      number('MaxAgeSeconds', 'Cache duration in seconds'),
    ], true)],
  },
  acl: {
    root: 'AccessControlPolicy',
    fields: [
      group('Owner', 'Bucket owner', [text('ID', 'Canonical user ID'), text('DisplayName', 'Display name')]),
      group('AccessControlList', 'Grants', [
        group('Grant', 'Grant', [group('Grantee', 'Grantee', principalFields), grantPermission], true),
      ]),
    ],
  },
  ownershipControls: {
    root: 'OwnershipControls',
    fields: [group('Rule', 'Ownership rule', [
      select('ObjectOwnership', 'Object ownership', ['BucketOwnerEnforced', 'BucketOwnerPreferred', 'ObjectWriter']),
    ])],
  },
  publicAccessBlock: {
    root: 'PublicAccessBlockConfiguration',
    fields: [
      check('BlockPublicAcls', 'Block new public ACLs'),
      check('IgnorePublicAcls', 'Ignore existing public ACLs'),
      check('BlockPublicPolicy', 'Block new public bucket policies'),
      check('RestrictPublicBuckets', 'Restrict access through public bucket policies'),
    ],
  },
  encryption: {
    root: 'ServerSideEncryptionConfiguration',
    fields: [group('Rule', 'Encryption rule', [
      group('ApplyServerSideEncryptionByDefault', 'Encryption method', [
        select('SSEAlgorithm', 'Algorithm', ['AES256', 'aws:kms', 'aws:kms:dsse']),
        encryptionKey,
      ]),
      check('BucketKeyEnabled', 'Use S3 Bucket Key'),
    ])],
  },
  objectLock: {
    root: 'ObjectLockConfiguration',
    fields: [
      select('ObjectLockEnabled', 'Object Lock status', ['Enabled']),
      group('Rule', 'Retention rule', [group('DefaultRetention', 'Default retention', [
        select('Mode', 'Retention mode', ['GOVERNANCE', 'COMPLIANCE']),
        number('Days', 'Retention days'),
        number('Years', 'Retention years'),
      ])]),
    ],
  },
  tagging: {
    root: 'Tagging',
    fields: [group('TagSet', 'Tags', [tag(true)])],
  },
  logging: {
    root: 'BucketLoggingStatus',
    fields: [group('LoggingEnabled', 'Logging destination', [
      text('TargetBucket', 'Destination bucket'),
      { ...text('TargetPrefix', 'Destination prefix'), allowEmpty: true },
      group('TargetGrants', 'Destination grants', [
        group('Grant', 'Grant', [group('Grantee', 'Grantee', principalFields), grantPermission], true),
      ]),
      group('TargetObjectKeyFormat', 'Log key format', [
        group('SimplePrefix', 'Simple prefix', []),
        group('PartitionedPrefix', 'Partitioned prefix', [
          select('PartitionDateSource', 'Partition date source', ['EventTime', 'DeliveryTime']),
        ]),
      ]),
    ])],
  },
  notification: {
    root: 'NotificationConfiguration',
    fields: [
      group('TopicConfiguration', 'SNS topic', [text('Topic', 'Topic ARN'), ...notificationFields], true),
      group('QueueConfiguration', 'SQS queue', [text('Queue', 'Queue ARN'), ...notificationFields], true),
      group('CloudFunctionConfiguration', 'Lambda function', [text('CloudFunction', 'Function ARN'), ...notificationFields], true),
      group('EventBridgeConfiguration', 'EventBridge', []),
    ],
  },
  replication: {
    root: 'ReplicationConfiguration',
    fields: [
      text('Role', 'IAM role ARN'),
      group('Rule', 'Replication rule', [
        text('ID', 'Rule ID'),
        number('Priority', 'Priority'),
        select('Status', 'Status', status),
        group('Filter', 'Filter', [prefix, tag(), group('And', 'Combined filters', [prefix, tag(true)])]),
        text('Prefix', 'Legacy key prefix'),
        group('DeleteMarkerReplication', 'Delete marker replication', [select('Status', 'Status', status)]),
        group('ExistingObjectReplication', 'Existing object replication', [select('Status', 'Status', status)]),
        group('SourceSelectionCriteria', 'Source selection', [
          group('SseKmsEncryptedObjects', 'SSE-KMS objects', [select('Status', 'Status', status)]),
          group('ReplicaModifications', 'Replica modifications', [select('Status', 'Status', status)]),
        ]),
        group('Destination', 'Destination', [
          text('Bucket', 'Bucket ARN'),
          text('Account', 'Account ID'),
          storageClass,
          group('AccessControlTranslation', 'Replica ownership', [select('Owner', 'Owner', ['Destination'])]),
          group('EncryptionConfiguration', 'Replica encryption', [text('ReplicaKmsKeyID', 'Replica KMS key ARN')]),
          group('Metrics', 'Metrics', [
            select('Status', 'Status', status),
            group('EventThreshold', 'Event threshold', [number('Minutes', 'Minutes')]),
          ]),
          group('ReplicationTime', 'Replication Time Control', [
            select('Status', 'Status', status),
            group('Time', 'Time limit', [number('Minutes', 'Minutes')]),
          ]),
        ]),
      ], true),
    ],
  },
  website: {
    root: 'WebsiteConfiguration',
    fields: [
      group('IndexDocument', 'Index document', [text('Suffix', 'File name')]),
      group('ErrorDocument', 'Error document', [text('Key', 'Object key')]),
      group('RedirectAllRequestsTo', 'Redirect all requests', websiteRedirect),
      group('RoutingRules', 'Routing rules', [
        group('RoutingRule', 'Routing rule', [
          group('Condition', 'Condition', [
            text('HttpErrorCodeReturnedEquals', 'HTTP error code'),
            text('KeyPrefixEquals', 'Key prefix'),
          ]),
          group('Redirect', 'Redirect', [
            ...websiteRedirect,
            text('HttpRedirectCode', 'HTTP redirect code'),
            text('ReplaceKeyPrefixWith', 'Replacement prefix'),
            text('ReplaceKeyWith', 'Replacement key'),
          ]),
        ], true),
      ]),
    ],
  },
  requestPayment: {
    root: 'RequestPaymentConfiguration',
    fields: [select('Payer', 'Payer', ['BucketOwner', 'Requester'])],
  },
}
