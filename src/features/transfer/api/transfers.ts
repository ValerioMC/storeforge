import { open, save } from '@tauri-apps/plugin-dialog'
import { desktop, isTauri } from '../../../common/tauri'
import type { Location } from '../../browser/model/objects'
import type { Comparison, Integrity, Job, Transfer, Usage } from '../model/transfer'

export const transfersApi = {
  jobs: () => isTauri() ? desktop<Job[]>('list_jobs') : Promise.resolve([]),
  start: (transfer: Transfer) => desktop<string>('start_transfer', { transfer }),
  cancel: (id: string) => desktop<void>('cancel_job', { id }),
  compare: (source: Location, target: Location) => desktop<Comparison>('compare_locations', { source, target }),
  usage: (location: Location, includeVersions: boolean) => desktop<Usage>('location_usage', { location, includeVersions }),
  integrity: (location: Location) => desktop<Integrity>('check_integrity', { location }),
  chooseFiles: () => open({ multiple: true, title: 'Choose files to upload' }),
  chooseDirectory: () => open({ directory: true, title: 'Choose download folder' }),
  saveFile: (name: string) => save({ defaultPath: name, title: 'Save downloaded file' }),
}
