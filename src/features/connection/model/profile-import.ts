import { endpointSettings, validCoordinates, type ProfileFields, type Provider } from './connection'

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
    const values = input.settings
    for (const key of ['tenant', 'account', 'realm', 'caCertificate'] as const) {
      const value = values[key]
      if (typeof value === 'string') settings[key] = value
      else if (value !== undefined) throw new Error('Invalid endpoint settings.')
    }
    for (const key of ['defaultEndpoint', 'browserEnabled', 'managerEnabled'] as const) {
      const value = values[key]
      if (typeof value === 'boolean') settings[key] = value
      else if (value !== undefined) throw new Error('Invalid workspace settings.')
    }
    for (const key of ['standardTags', 'administrativeTags'] as const) {
      const value = values[key]
      if (isStringList(value)) settings[key] = value
      else if (value !== undefined) throw new Error('Invalid profile tags.')
    }
    const coordinates = values.coordinates
    if (coordinates != null) {
      if (!isRecord(coordinates) || !validCoordinates(coordinates.latitude, coordinates.longitude) ||
        typeof coordinates.latitude !== 'number' || typeof coordinates.longitude !== 'number') throw new Error('Invalid endpoint coordinates.')
      settings.coordinates = { latitude: coordinates.latitude, longitude: coordinates.longitude }
    }
    const preferred = values.defaultWorkspace
    if (preferred === 'browser' || preferred === 'manager' || preferred === 'cephAdmin' || preferred === 'admin') {
      settings.defaultWorkspace = preferred
    } else if (preferred !== undefined && preferred !== null) {
      throw new Error('Invalid default workspace.')
    }
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
