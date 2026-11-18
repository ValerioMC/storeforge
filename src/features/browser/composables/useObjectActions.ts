import { ref } from 'vue'
import { errorMessage } from '../../../common/tauri'
import { transfersApi } from '../../transfer/api/transfers'
import { useJobs } from '../../transfer/store/jobs'
import type { Transfer } from '../../transfer/model/transfer'
import type { Location, ObjectId } from '../model/objects'

interface Clipboard {
  source: Location
  objects: ObjectId[]
  move: boolean
}

export function useObjectActions(onJob: (id: string) => void) {
  const jobs = useJobs()
  const clipboard = ref<Clipboard>()
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
    notice.value = 'Operation started. Follow its progress in Activity.'
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

  async function archive(connection: string, objects: ObjectId[]): Promise<void> {
    await run(async () => {
      const target = await transfersApi.saveFile('objects.zip')
      if (target) await enqueue({ kind: 'archive', connection, objects, target })
    })
  }

  function stage(source: Location, objects: ObjectId[], move: boolean): void {
    clipboard.value = { source, objects, move }
    notice.value = `${objects.length} ${objects.length === 1 ? 'object' : 'objects'} ready to ${move ? 'move' : 'copy'}. Open the destination and choose Paste.`
  }

  async function paste(target: Location): Promise<void> {
    const current = clipboard.value
    if (!current) return
    await run(async () => {
      await enqueue({ kind: 'copy', source: current.source, objects: current.objects, target, moveSource: current.move, overwrite: false })
      if (current.move) clipboard.value = undefined
    })
  }

  return { clipboard, failure, notice, run, enqueue, upload, uploadPaths, download, archive, stage, paste }
}
