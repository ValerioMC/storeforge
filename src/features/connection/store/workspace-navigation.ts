import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import { isWorkspaceRoute, type WorkspaceRoute } from '../model/workspace'

const storageKey = 'storeforge.navigation'
const historyLimit = 50

export const useWorkspaceNavigation = defineStore('workspace-navigation', () => {
  const routes = ref<WorkspaceRoute[]>([])
  const index = ref(0)
  const current = computed(() => routes.value[index.value])
  const previous = computed(() => routes.value[index.value - 1])
  const canBack = computed(() => index.value > 0)
  const canForward = computed(() => index.value < routes.value.length - 1)

  function initialize(fallback: WorkspaceRoute, allow: (route: WorkspaceRoute) => boolean = () => true): void {
    const history = readHistory()
    if (history && history.routes.every(allow)) {
      routes.value = history.routes
      index.value = history.index
    } else {
      reset(fallback)
    }
  }

  function reset(route: WorkspaceRoute): void {
    routes.value = [route]
    index.value = 0
    persist()
  }

  function go(route: WorkspaceRoute): void {
    if (JSON.stringify(current.value) === JSON.stringify(route)) return
    routes.value = [...routes.value.slice(0, index.value + 1), route].slice(-historyLimit)
    index.value = routes.value.length - 1
    persist()
  }

  function replace(route: WorkspaceRoute): void {
    routes.value = routes.value.map((entry, position) => position === index.value ? route : entry)
    if (!routes.value.length) routes.value = [route]
    persist()
  }

  function back(): void {
    if (canBack.value) {
      index.value--
      persist()
    }
  }

  function forward(): void {
    if (canForward.value) {
      index.value++
      persist()
    }
  }

  function persist(): void {
    if (current.value?.connectionId !== 'demo') {
      window.localStorage.setItem(storageKey, JSON.stringify({ routes: routes.value, index: index.value }))
    }
  }

  return { current, previous, canBack, canForward, initialize, reset, go, replace, back, forward }
})

interface StoredHistory {
  routes: WorkspaceRoute[]
  index: number
}

function readHistory(): StoredHistory | undefined {
  try {
    const value: unknown = JSON.parse(window.localStorage.getItem(storageKey) ?? 'null')
    if (!value || typeof value !== 'object' || !('routes' in value) || !Array.isArray(value.routes) ||
      !('index' in value) || typeof value.index !== 'number' || !Number.isInteger(value.index) ||
      value.index < 0 || value.index >= value.routes.length || value.routes.length > historyLimit ||
      !value.routes.every(isWorkspaceRoute)) return undefined
    return { routes: value.routes, index: value.index }
  } catch {
    return undefined
  }
}
