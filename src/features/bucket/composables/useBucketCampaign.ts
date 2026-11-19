import { computed, ref, shallowRef } from 'vue'
import { applyCampaignItem, previewCampaignTarget, type CampaignGateway, type CampaignItem, type CampaignPatch, type CampaignTarget } from '../model/campaign'
import type { WorkbenchRow } from '../model/workbench'

export interface CampaignRequest { row: WorkbenchRow; patch: CampaignPatch }

export function useBucketCampaign(gateway: CampaignGateway, onResult: (row: WorkbenchRow, item: CampaignItem) => void) {
  const targets = shallowRef<CampaignTarget[]>([])
  const confirmed = ref(new Set<string>())
  const stage = ref<'draft' | 'preview' | 'results'>('draft')
  const running = ref(false)
  const progress = ref({ complete: 0, total: 0 })
  const eligible = computed(() => targets.value.filter(target => target.row.profile.id !== 'demo' && target.items.some(item => item.status === 'ready')))
  const selected = computed(() => eligible.value.filter(target => confirmed.value.has(target.row.key)))
  const retryable = computed(() => targets.value.filter(target => target.items.some(canRetry)))

  function setConfirmed(key: string, checked: boolean): void {
    const values = new Set(confirmed.value)
    if (checked) values.add(key)
    else values.delete(key)
    confirmed.value = values
  }

  function confirmEligible(): void {
    confirmed.value = new Set(eligible.value.map(target => target.row.key))
  }

  async function preview(requests: readonly CampaignRequest[], existing: readonly CampaignTarget[] = []): Promise<void> {
    if (running.value) return
    running.value = true
    progress.value = { complete: 0, total: requests.length }
    confirmed.value = new Set()
    try {
      let cursor = 0
      const loaded = new Map<string, CampaignTarget>()
      async function loadNext(): Promise<void> {
        for (let request = requests[cursor++]; request; request = requests[cursor++]) {
          const target = await previewCampaignTarget(request.row, request.patch, gateway)
          loaded.set(request.row.key, target)
          progress.value.complete++
        }
      }
      await Promise.all(Array.from({ length: Math.min(4, requests.length) }, loadNext))
      targets.value = existing.length ? existing.map(target => {
        const retry = loaded.get(target.row.key)
        if (!retry) return target
        const bySection = new Map(retry.items.map(item => [item.section, item]))
        return { ...target, items: target.items.map(item => bySection.get(item.section) ?? item) }
      }) : requests.map(({ row }) => loaded.get(row.key)!)
      stage.value = 'preview'
    } finally {
      running.value = false
    }
  }

  async function apply(): Promise<void> {
    if (running.value || stage.value !== 'preview' || !selected.value.length) return
    const chosen = selected.value.slice()
    const scope = new Set(chosen.map(target => target.row.key))
    targets.value = targets.value.map(target => scope.has(target.row.key) ? target : {
      ...target, items: target.items.map(item => item.status === 'ready'
        ? { ...item, status: 'skipped', summary: 'Target was not confirmed.' } : item),
    })
    running.value = true
    stage.value = 'results'
    progress.value = { complete: 0, total: chosen.length }
    try {
      for (const target of chosen) {
        const items = target.items.slice()
        for (const [index, item] of target.items.entries()) {
          if (item.status !== 'ready') continue
          const result = await applyCampaignItem(target.row, item, gateway)
          onResult(target.row, result)
          items[index] = result
          targets.value = targets.value.map(entry => entry.row.key === target.row.key ? { ...target, items: [...items] } : entry)
        }
        progress.value.complete++
      }
    } finally {
      confirmed.value = new Set()
      running.value = false
    }
  }

  async function retry(): Promise<void> {
    const failures = retryable.value
    await preview(failures.map(target => {
      const failedSections = new Set(target.items.filter(canRetry).map(item => item.section))
      return {
        row: target.row,
        patch: { sections: target.patch.sections.filter(intent => failedSections.has(intent.section)),
          quota: failedSections.has('quota') ? target.patch.quota : undefined, usage: failedSections.has('usage') && target.patch.usage },
      }
    }), targets.value)
  }

  function edit(): void {
    if (!running.value) stage.value = 'draft'
  }

  return { targets, confirmed, stage, running, progress, eligible, selected, retryable, setConfirmed, confirmEligible, preview, apply, retry, edit }
}

function canRetry(item: CampaignItem): boolean {
  return item.status === 'denied' || item.status === 'error' || item.status === 'partial'
}
