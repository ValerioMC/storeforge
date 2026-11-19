import { shallowRef } from 'vue'
import { classificationKey, type InventorySource, type WorkbenchRow } from '../model/workbench'

const storageKey = 'storeforge.bucket-classifications'
const emptyTags: readonly string[] = []

export function useBucketClassifications(source: InventorySource) {
  const values = shallowRef(readClassifications())

  function tags(row: WorkbenchRow): readonly string[] {
    return values.value.get(classificationKey(source, row)) ?? emptyTags
  }

  function saveTags(row: WorkbenchRow, text: string): void {
    const key = classificationKey(source, row)
    const updated = new Map(readClassifications())
    const items = [...new Set(text.split(',').map(item => item.trim()).filter(Boolean))]
    if (items.length) updated.set(key, items)
    else updated.delete(key)
    window.localStorage.setItem(storageKey, JSON.stringify([...updated]))
    values.value = updated
  }

  return { tags, saveTags }
}

function readClassifications(): Map<string, readonly string[]> {
  try {
    const stored: unknown = JSON.parse(window.localStorage.getItem(storageKey) ?? '[]')
    const entries = Array.isArray(stored) ? stored.filter((entry): entry is [string, string[]] =>
      Array.isArray(entry) && typeof entry[0] === 'string' && Array.isArray(entry[1]) &&
      entry[1].every((item: unknown) => typeof item === 'string')) : []
    return new Map(entries)
  } catch {
    return new Map()
  }
}
