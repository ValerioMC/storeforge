import catalog from './operations.json'

export type Protocol = 'iam' | 'sns' | 'rgw'

interface Field {
  key: string
  label: string
  required: boolean
  multiline: boolean
}

export interface Operation {
  id: string
  group: string
  protocol: Protocol
  label: string
  action: string
  fields: Field[]
  write: boolean
  method?: string
}

export const operations = catalog as Operation[]

export interface AdminRequest {
  operation: string
  values: Record<string, string>
  confirmed: boolean
}
