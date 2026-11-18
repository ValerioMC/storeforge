import { browserLocationKey, defaultOptions, type BrowserProfile, type BrowserSort, type DetailColumn, type ListingOptions, type ResizableColumn } from './listing'
import type { Location } from './objects'

export interface ListingPreferences {
  recursive: boolean
  technical: boolean
  advanced: boolean
  options: ListingOptions
  sort: BrowserSort
  details: DetailColumn[]
  columnWidths: Partial<Record<ResizableColumn, number>>
}

export function defaultPreferences(): ListingPreferences {
  return {
    recursive: false, technical: false, advanced: false, options: defaultOptions(),
    sort: { column: 'provider', ascending: true }, details: [], columnWidths: {},
  }
}

function stored<T>(key: string, fallback: T): T {
  try {
    return JSON.parse(window.localStorage.getItem(key) ?? 'null') as T ?? fallback
  } catch {
    return fallback
  }
}

function preferenceKey(profile: BrowserProfile): string {
  return `storeforge.browser.views.${profile}`
}

export function readListingPreferences(profile: BrowserProfile, key: string): ListingPreferences {
  const views = stored<Record<string, ListingPreferences>>(preferenceKey(profile), {})
  const value = views[key]
  const defaults = defaultPreferences()
  return value ? { ...defaults, ...value, options: { ...defaults.options, ...value.options } } : defaults
}

export function writeListingPreferences(profile: BrowserProfile, key: string, value: ListingPreferences): void {
  const storageKey = preferenceKey(profile)
  const views = new Map(Object.entries(stored<Record<string, ListingPreferences>>(storageKey, {})))
  views.delete(key)
  views.set(key, value)
  window.localStorage.setItem(storageKey, JSON.stringify(Object.fromEntries([...views].slice(-80))))
}

const favoritesKey = 'storeforge.browser.favorites'

function isLocation(value: unknown): value is Location {
  return typeof value === 'object' && value !== null &&
    'connection' in value && typeof value.connection === 'string' && !!value.connection &&
    'bucket' in value && typeof value.bucket === 'string' && !!value.bucket &&
    'prefix' in value && typeof value.prefix === 'string'
}

export function readFavorites(): Location[] {
  const list = stored<unknown>(favoritesKey, [])
  return Array.isArray(list) ? list.filter(isLocation) : []
}

export function matchesFavorite(location: Location, profile: string, query: string): boolean {
  return [profile, location.bucket, location.prefix].some(value => value.toLowerCase().includes(query.toLowerCase()))
}

export function writeFavorite(location: Location): Location[] {
  const favorites = readFavorites()
  const key = browserLocationKey(location)
  const index = favorites.findIndex(item => browserLocationKey(item) === key)
  if (index < 0) favorites.push(location)
  else favorites.splice(index, 1)
  window.localStorage.setItem(favoritesKey, JSON.stringify(favorites))
  return favorites
}
