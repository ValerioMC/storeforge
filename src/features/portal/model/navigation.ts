export const portalPages = {
  overview: 'Overview',
  spaces: 'Storage spaces',
  collaborators: 'Collaborators',
  keys: 'Access keys',
  requests: 'Requests',
} as const
export type PortalPage = keyof typeof portalPages
export type SpaceView = 'files' | 'trash' | 'statistics' | 'settings' | 'sharing'
export type FileView = 'details' | 'versions'

export interface PortalLocation {
  page: PortalPage
  serviceId?: string
  projectId?: number
  spaceId?: string
  personId?: number
  view?: SpaceView
  folder?: string
  object?: string
  fileView?: FileView
}

export function isPortalLocation(value: object): value is PortalLocation {
  if (!('page' in value) || typeof value.page !== 'string' || !Object.hasOwn(portalPages, value.page)) return false
  if ('serviceId' in value && (typeof value.serviceId !== 'string' || !value.serviceId)) return false
  if ('projectId' in value && (typeof value.projectId !== 'number' || !Number.isSafeInteger(value.projectId) || value.projectId < 1 || !('serviceId' in value))) return false
  if ('personId' in value && (value.page !== 'collaborators' || !('projectId' in value) || typeof value.personId !== 'number' || !Number.isSafeInteger(value.personId) || value.personId < 1)) return false
  if ('spaceId' in value && (value.page !== 'spaces' || !('projectId' in value) || typeof value.spaceId !== 'string' || !value.spaceId)) return false
  if ('view' in value && (!('spaceId' in value) || !['files', 'trash', 'statistics', 'settings', 'sharing'].some(view => view === value.view))) return false
  if ('folder' in value && (!('spaceId' in value) || typeof value.folder !== 'string')) return false
  if ('object' in value && (!('spaceId' in value) || typeof value.object !== 'string' || !value.object)) return false
  return !('fileView' in value) || 'object' in value && (value.fileView === 'details' || value.fileView === 'versions')
}

export function readPortalLocation(search: string): PortalLocation | undefined {
  const query = new URLSearchParams(search)
  const page = query.get('portal')
  if (page === null) return undefined
  const project = query.get('project')
  const person = query.get('person')
  const location = {
    page,
    ...Object.fromEntries(['serviceId', 'spaceId', 'view', 'folder', 'object', 'fileView']
      .flatMap(key => query.has(key) ? [[key, query.get(key)]] : [])),
    ...(person === null ? {} : { personId: Number(person) }),
    ...(project === null ? {} : { projectId: Number(project) }),
  }
  return isPortalLocation(location) ? location : undefined
}

export function writePortalLocation(url: URL, location?: PortalLocation): void {
  for (const key of ['portal', 'serviceId', 'project', 'person', 'spaceId', 'view', 'folder', 'object', 'fileView']) url.searchParams.delete(key)
  if (!location) return
  url.searchParams.set('portal', location.page)
  for (const [key, value] of Object.entries({
    serviceId: location.serviceId, project: location.projectId, person: location.personId, spaceId: location.spaceId, view: location.view,
    folder: location.folder, object: location.object, fileView: location.fileView,
  })) {
    if (value !== undefined) url.searchParams.set(key, String(value))
  }
}
