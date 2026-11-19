export interface OpenStackCloudSettings {
  name: string
  authUrl: string
  defaultDomain: string
  managedEndpointsOnly: boolean
}

export interface OpenStackCloud extends OpenStackCloudSettings {
  id: string
}

export interface OpenStackProject {
  id: string
  name: string
  domainId: string | null
}

export interface OpenStackSession {
  identity: string
  expiresAt: string
  projects: OpenStackProject[]
}

export interface OpenStackCloudStatus {
  cloud: OpenStackCloud
  session: OpenStackSession | null
  reconnectRequired: boolean
}

export interface OpenStackTarget {
  id: string
  region: string
  interface: 'public' | 'internal'
  endpoint: string
  managed: boolean
  allowed: boolean
}

export interface OpenStackProjectAccess {
  id: string
  targets: OpenStackTarget[]
  accessKeys: string[]
}

export interface OpenStackActivation {
  discoveryId: string
  targetId: string
  accessKey: string
}

export type OpenStackAuthentication =
  | { method: 'password'; username: string; domain: string; password: string }
  | { method: 'token'; token: string }

export interface OpenStackProfile {
  cloudId: string
  projectName: string
  expiresAt: string
}

export function onlyChoice<T>(options: readonly T[]): T | undefined {
  return options.length === 1 ? options[0] : undefined
}

export function needsOpenStackSignIn(error: unknown): boolean {
  return typeof error === 'object' && error !== null && 'kind' in error && error.kind === 'reauthenticate'
}
