import { endpointSettings, type ProfileFields, type Provider } from './connection'

export function parseProfile(source: string): ProfileFields {
  const input: unknown = JSON.parse(source)
  const invalid = () => new Error('The profile file requires a name, provider, endpoint, region, addressing mode and bucket list.')
  if (!isRecord(input)) throw invalid()
  const { name, provider, endpoint, region, pathStyle, knownBuckets } = input
  if (typeof name !== 'string' || !isProvider(provider) || typeof endpoint !== 'string' ||
    typeof region !== 'string' || typeof pathStyle !== 'boolean' || !isStringList(knownBuckets)) throw invalid()

  const settings = endpointSettings()
  if (input.settings !== undefined) {
    if (!isRecord(input.settings)) throw new Error('Invalid endpoint settings.')
    const { tenant, account, realm, defaultEndpoint, standardTags, administrativeTags, caCertificate } = input.settings
    for (const key of ['tenant', 'account', 'realm', 'caCertificate'] as const) {
      const value = input.settings[key]
      if (value !== undefined && typeof value !== 'string') throw new Error('Invalid endpoint settings.')
    }
    if ((defaultEndpoint !== undefined && typeof defaultEndpoint !== 'boolean') ||
      (standardTags !== undefined && !isStringList(standardTags)) ||
      (administrativeTags !== undefined && !isStringList(administrativeTags))) throw new Error('Invalid endpoint settings.')
    Object.assign(settings, { tenant: tenant ?? '', account: account ?? '', realm: realm ?? '', defaultEndpoint: defaultEndpoint ?? false,
      standardTags: standardTags ?? [], administrativeTags: administrativeTags ?? [], caCertificate: caCertificate ?? '' })
  }
  return { name, provider, endpoint, region, pathStyle, knownBuckets, settings }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function isProvider(value: unknown): value is Provider {
  return value === 'aws' || value === 'ceph' || value === 'compatible'
}

function isStringList(value: unknown): value is string[] {
  return Array.isArray(value) && value.every(item => typeof item === 'string')
}
