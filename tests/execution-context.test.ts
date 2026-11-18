import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { desktop } from '../src/common/tauri'
import { objectsApi } from '../src/features/browser/api/objects'
import type { ObjectPage } from '../src/features/browser/model/objects'
import { useBrowser } from '../src/features/browser/store/browser'

vi.mock('../src/common/tauri', () => ({
  desktop: vi.fn(),
  isTauri: () => true,
  errorMessage: String,
}))

function objectPage(key: string): ObjectPage {
  return { cursor: null, rows: [{ key, folder: false, size: 1, modified: null, storageClass: null, etag: null }] }
}

describe('execution context', () => {
  beforeEach(() => {
    vi.resetAllMocks()
    setActivePinia(createPinia())
  })

  it('keeps same-named buckets isolated when a tenant read finishes late', async () => {
    let resolveOldRead!: (page: ObjectPage) => void
    const oldRead = new Promise<ObjectPage>(resolve => { resolveOldRead = resolve })
    vi.mocked(desktop).mockImplementation((command, args) => {
      if (command === 'list_buckets') return Promise.resolve([{ name: 'archive', createdAt: null }])
      if (command === 'list_objects') {
        const { query } = args as { query: { location: { connection: string } } }
        return query.location.connection === 'tenant-a' ? oldRead : Promise.resolve(objectPage('tenant-b.txt'))
      }
      return Promise.resolve()
    })
    const browser = useBrowser()
    await browser.changeConnection('tenant-a')
    const oldNavigation = browser.navigate('archive')
    await browser.changeConnection('tenant-b')
    await browser.navigate('archive')
    resolveOldRead(objectPage('tenant-a.txt'))
    await oldNavigation

    expect(browser.rows.map(row => row.key)).toEqual(['tenant-b.txt'])
    await objectsApi.folder(browser.location.connection, browser.bucket, 'new/')
    expect(desktop).toHaveBeenLastCalledWith('create_folder', { connection: 'tenant-b', bucket: 'archive', key: 'new/' })
  })
})
