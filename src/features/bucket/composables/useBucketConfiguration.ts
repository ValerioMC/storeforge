import { computed, shallowReactive } from 'vue'
import { errorMessage } from '../../../common/tauri'
import type { Configuration, ConfigurationChange } from '../model/bucket'
import { parseConfiguration, readConfiguration, type ConfigurationReading } from '../model/workspace'

interface Section {
  original: string
  draft: string
  reading?: ConfigurationReading
  busy: boolean
  writing: boolean
  error: string
  notice: string
}

export function useBucketConfiguration(request: (section: Configuration, change: ConfigurationChange) => Promise<string>) {
  const sections = shallowReactive(new Map<Configuration, Section>())
  const dirty = computed(() => Array.from(sections.values()).some(section => section.draft !== section.original))
  const writing = computed(() => Array.from(sections.values()).some(section => section.writing))

  function state(section: Configuration): Section {
    return sections.get(section) ?? { original: '', draft: '', busy: false, writing: false, error: '', notice: '' }
  }

  function update(section: Configuration, changes: Partial<Section>): void {
    sections.set(section, { ...state(section), ...changes })
  }

  function edit(section: Configuration, draft: string): void {
    update(section, { draft, error: '', notice: '' })
  }

  async function refresh(section: Configuration, force = false): Promise<void> {
    const prior = state(section)
    if (prior.busy || (!force && prior.reading)) return
    update(section, { busy: true, error: '', notice: '' })
    await reload(section)
  }

  async function reload(section: Configuration, saved?: string): Promise<void> {
    const reading = await readConfiguration(() => request(section, { action: 'read' }), '', section)
    const prior = state(section)
    if (reading.status === 'error') {
      update(section, { reading, busy: false, notice: saved === undefined ? '' : 'Saved. The provider could not be read back.' })
      return
    }
    const preserveDraft = saved === undefined && prior.draft !== prior.original
    update(section, {
      reading, busy: false, original: reading.document,
      draft: preserveDraft ? prior.draft : reading.document, notice: saved === undefined ? '' : 'Saved.',
    })
  }

  async function refreshLoaded(): Promise<void> {
    await Promise.all(Array.from(sections.keys(), section => refresh(section, true)))
  }

  async function write(section: Configuration, change: Extract<ConfigurationChange, { action: 'write' }> | { action: 'remove' }): Promise<boolean> {
    if (state(section).busy) return false
    update(section, { busy: true, writing: true, error: '', notice: '' })
    try {
      if (change.action === 'write') parseConfiguration(section, change.document)
      await request(section, change)
      const saved = change.action === 'write' ? change.document : ''
      update(section, { original: saved, draft: saved })
      await reload(section, saved)
      return true
    } catch (failure) {
      const error = errorMessage(failure)
      await reload(section)
      update(section, { error })
      return false
    } finally {
      update(section, { writing: false })
    }
  }

  function discard(): void {
    for (const [section, value] of sections) edit(section, value.original)
  }

  return { state, dirty, writing, edit, refresh, refreshLoaded, write, discard }
}
