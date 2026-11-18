import type { ProfileFields } from './connection'
import { parseProfile } from './profile-import'

export type SetupStage = 'endpoint' | 'workspaces' | 'credentials' | 'review'

export const setupStages: ReadonlyArray<{ value: SetupStage; label: string }> = [
  { value: 'endpoint', label: 'Endpoint' },
  { value: 'workspaces', label: 'Workspaces' },
  { value: 'credentials', label: 'Credentials' },
  { value: 'review', label: 'Review' },
]

const key = 'storeforge.connection-setup'

export function pausedSetup(): { profile: ProfileFields; stage: SetupStage } | undefined {
  try {
    const stored: unknown = JSON.parse(window.localStorage.getItem(key) ?? 'null')
    if (!stored || typeof stored !== 'object' || !('profile' in stored)) return undefined
    const stage = 'stage' in stored && (stored.stage === 'workspaces' || stored.stage === 'credentials') ? stored.stage : 'endpoint'
    return { profile: parseProfile(JSON.stringify(stored.profile)), stage }
  } catch {
    return undefined
  }
}

export function pauseSetup(profile: ProfileFields, stage: SetupStage): void {
  const { name, provider, endpoint, region, pathStyle, knownBuckets, settings } = profile
  window.localStorage.setItem(key, JSON.stringify({
    profile: { name, provider, endpoint, region, pathStyle, knownBuckets, settings },
    stage: stage === 'review' ? 'credentials' : stage,
  }))
}

export function discardSetup(): void {
  window.localStorage.removeItem(key)
}
