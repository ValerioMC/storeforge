import { ref, shallowRef } from 'vue'
import { errorMessage } from '../../../common/tauri'
import { transfersApi } from '../../transfer/api/transfers'
import { useJobs } from '../../transfer/store/jobs'
import type { Transfer } from '../../transfer/model/transfer'
import type { Location, ObjectId } from '../model/objects'

interface Clipboard {
  source: Location
  objects: ObjectId[]
  folders: string[]
  revision: string | null
  move: boolean
}

const clipboard = shallowRef<Clipboard>()

export function useObjectActions(onJob: (id: string) => void) {
  const jobs = useJobs()
  const failure = ref('')
  const notice = ref('')

  async function run(action: () => Promise<void>): Promise<void> {
    failure.value = ''
    notice.value = ''
    try {
      await action()
    } catch (error) {
      failure.value = errorMessage(error)
    }
  }

  async function enqueue(transfer: Transfer): Promise<void> {
    const id = await jobs.start(transfer)
    onJob(id)
    notice.value = 'Operation started.'
  }

  async function upload(target: Location): Promise<void> {
    await run(async () => {
      const paths = await transfersApi.chooseFiles()
      if (paths?.length) await enqueue({ kind: 'upload', target, paths, overwrite: false })
    })
  }

  async function uploadPaths(target: Location, paths: string[]): Promise<void> {
    if (paths.length) await run(() => enqueue({ kind: 'upload', target, paths, overwrite: false }))
  }

  async function download(connection: string, objects: ObjectId[]): Promise<void> {
    await run(async () => {
      if (objects.length > 1) {
        const target = await transfersApi.chooseDirectory()
        if (target) await enqueue({ kind: 'download', connection, objects, target, directory: true })
      } else {
        const object = objects[0]
        if (!object) return
        const basename = object.key.split('/').at(-1) || 'download'
        const target = await transfersApi.saveFile(basename.replace(/[\\:*?"<>|]/g, '_'))
        if (target) await enqueue({ kind: 'download', connection, objects, target, directory: false })
      }
    })
  }

  async function archive(source: Location, objects: ObjectId[], folders: string[]): Promise<void> {
    await run(async () => {
      const target = await transfersApi.saveFile('objects.zip')
      if (target) await enqueue({ kind: 'archive', ...source, objects, folders, target })
    })
  }

  function stage(source: Location, revision: string | null, objects: ObjectId[], folders: string[], move: boolean): void {
    clipboard.value = { source: { ...source }, revision, objects: objects.slice(), folders: folders.slice(), move }
    notice.value = 'Open the destination and choose Paste.'
  }

  async function paste(target: Location, sourceRevision: string | null | undefined): Promise<void> {
    const current = clipboard.value
    if (!current) return
    await run(async () => {
      if (current.revision !== sourceRevision) throw new Error('The source connection has changed. Select the source items again.')
      await enqueue({ kind: 'copy', source: current.source, objects: current.objects, folders: current.folders, target, moveSource: current.move, overwrite: false })
      if (current.move && clipboard.value === current) clipboard.value = undefined
    })
  }

  return { clipboard, failure, notice, run, enqueue, upload, uploadPaths, download, archive, stage, paste }
}
