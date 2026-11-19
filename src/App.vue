<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { Activity, ArrowLeft, ArrowRight, ChevronRight, CircleHelp, PanelLeftClose, PanelLeftOpen, Server } from 'lucide-vue-next'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { errorMessage, isTauri } from './common/tauri'
import Sidebar from './components/shell/Sidebar.vue'
import WindowControls from './components/shell/WindowControls.vue'
import AppSelect from './components/ui/AppSelect.vue'
import ConfirmDialog from './components/ui/ConfirmDialog.vue'
import FadeRegion from './components/ui/FadeRegion.vue'
import Notice from './components/ui/Notice.vue'
import AdminWorkspace from './features/administration/components/AdminWorkspace.vue'
import BrowserWorkspace from './features/browser/components/BrowserWorkspace.vue'
import type { Location } from './features/browser/model/objects'
import { demoConnection } from './features/browser/model/demo'
import { useBrowser } from './features/browser/store/browser'
import BucketWorkbench from './features/bucket/components/BucketWorkbench.vue'
import BucketRulesWorkspace from './features/bucket/components/BucketRulesWorkspace.vue'
import ManagerWorkspace from './features/bucket/components/ManagerWorkspace.vue'
import NewBucketDialog from './features/bucket/components/NewBucketDialog.vue'
import ConnectionEditor from './features/connection/components/ConnectionEditor.vue'
import ConnectionsWorkspace from './features/connection/components/ConnectionsWorkspace.vue'
import WorkspaceHelp from './features/connection/components/WorkspaceHelp.vue'
import WorkspacePreferences from './features/connection/components/WorkspacePreferences.vue'
import { identityAvailable, providerNames, type Connection } from './features/connection/model/connection'
import { pausedSetup } from './features/connection/model/setup-draft'
import { defaultRoute, isWorkspaceRoute, routeTitle, workspaceAvailable, workspaceNames, type WorkspaceDestination, type WorkspaceRoute } from './features/connection/model/workspace'
import { useConnections } from './features/connection/store/connections'
import { useWorkspaceNavigation } from './features/connection/store/workspace-navigation'
import MonitoringWorkspace from './features/monitoring/components/MonitoringWorkspace.vue'
import ActivityWorkspace from './features/transfer/components/ActivityWorkspace.vue'
import ToolsWorkspace from './features/transfer/components/ToolsWorkspace.vue'
import { useJobs } from './features/transfer/store/jobs'

const connections = useConnections()
const browser = useBrowser()
const jobs = useJobs()
const navigation = useWorkspaceNavigation()
const route = computed(() => navigation.current ?? defaultRoute())
const editor = ref(false)
const editedProfile = ref<Connection>()
const hasDraft = ref(!!pausedSetup())
const creatingBucket = ref(false)
const help = ref(false)
const sidebar = ref(window.localStorage.getItem('storeforge.sidebar') !== 'closed')
const compact = ref(window.localStorage.getItem('storeforge.density') === 'compact')
const showTags = ref(window.localStorage.getItem('storeforge.selectorTags') === 'true')
const startup = ref(window.localStorage.getItem('storeforge.startupContext') ?? '')
const startError = ref('')
const browserView = ref<InstanceType<typeof BrowserWorkspace>>()
const managerView = ref<InstanceType<typeof ManagerWorkspace>>()
const workbenchView = ref<InstanceType<typeof BucketWorkbench>>()
const bucketEditor = computed(() => workbenchRoute.value ? workbenchView.value : managerView.value)
const departure = ref<() => void>()
const demo = computed(() => connections.activeId === 'demo')
const contextOptions = computed(() => [
  ...connections.profiles.map(profile => ({ value: profile.id, label: profileLabel(profile) })),
  { value: 'demo', label: 'Asteria Research (demo)' },
])
const realmOptions = computed(() => connections.active ? [
  { value: connections.active.id, label: profileLabel(connections.active) },
  ...connections.realmEndpoints.map(profile => ({ value: profile.id, label: profileLabel(profile) })),
] : [])
const canBrowse = computed(() => workspaceAvailable(connections.active, 'browser'))
const canManage = computed(() => workspaceAvailable(connections.active, 'manager'))
const managerProfiles = computed(() => connections.profiles.filter(profile => workspaceAvailable(profile, 'manager')))
const managerRoute = computed(() => route.value.workspace === 'manager' ? route.value : undefined)
const monitoringRoute = computed(() => {
  const current = route.value
  if (current.workspace === 'manager' && (current.page === 'overview' || current.page === 'metrics')) return { workspace: current.workspace, pane: current.page, metric: current.metric }
  if (current.workspace === 'cephAdmin' && current.page !== 'resources' && current.page !== 'buckets') return { workspace: current.workspace, pane: current.page ?? 'overview', metric: current.metric }
  if (current.workspace === 'admin' && (current.page === 'overview' || current.page === 'metrics' || current.page === 'health')) return { workspace: current.workspace, pane: current.page, metric: current.metric }
  return undefined
})
const workbenchRoute = computed(() => {
  const current = route.value
  if (current.workspace === 'storageOps') return { source: current.workspace, pane: current.page, target: current.workbenchBucket, bucket: undefined } as const
  if (current.workspace === 'cephAdmin' && current.page === 'buckets') {
    return { source: current.workspace, pane: current.page, target: current.workbenchBucket, bucket: current.bucket } as const
  }
  return undefined
})
const workbenchProfiles = computed(() => {
  if (demo.value) return [demoConnection]
  if (workbenchRoute.value?.source === 'storageOps') return managerProfiles.value
  const active = connections.active
  return active && identityAvailable(active, 'cephAdmin') ? [active] : []
})
const workbenchLink = computed(() => {
  const url = new URL(window.location.href)
  url.hash = new URLSearchParams({ bucketWorkbench: JSON.stringify(route.value) }).toString()
  return url.href
})

const toolsLocation = computed(() => ({
  connection: browser.location.connection,
  bucket: managerRoute.value?.bucket ?? browser.bucket,
  prefix: managerRoute.value?.bucket ? (managerRoute.value.bucket === browser.bucket ? browser.prefix : '') : browser.prefix,
}))
const resource = computed(() => {
  const current = route.value
  if (current.workspace === 'browser') return current.prefix || current.bucket
  if (current.workspace === 'cephAdmin') return current.bucket
  return managerRoute.value?.bucket
})
const windowChrome = (window as Window & { __storeforgeWindowChrome?: 'overlay' | 'custom' }).__storeforgeWindowChrome
const dragRegion = windowChrome ? 'deep' : undefined
let timer: number | undefined
let unlistenClose: (() => void) | undefined
let closing = false
let mounted = false
let previousRunning = new Set<string>()
let browserContext = ''

watch([() => navigation.current, () => connections.profiles], () => {
  if (navigation.current) synchronize(navigation.current)
})
watch(sidebar, value => window.localStorage.setItem('storeforge.sidebar', value ? 'open' : 'closed'))
watch(compact, value => window.localStorage.setItem('storeforge.density', value ? 'compact' : 'comfortable'))
watch(showTags, value => window.localStorage.setItem('storeforge.selectorTags', String(value)))
watch(startup, value => window.localStorage.setItem('storeforge.startupContext', value))
watch(() => jobs.jobs, jobList => {
  const completed = jobList.some(job => previousRunning.has(job.id) && job.status !== 'running')
  previousRunning = new Set(jobList.filter(job => job.status === 'running').map(job => job.id))
  if (completed && browser.bucket) void browser.loadRows()
})

onMounted(() => {
  mounted = true
  void initialize()
  window.addEventListener('keydown', keydown)
  window.addEventListener('hashchange', followWorkbenchLink)
  window.addEventListener('beforeunload', beforeUnload)
  if (isTauri()) {
    timer = window.setInterval(() => void jobs.load(), 1200)
    void watchWindowClose()
  }
})

onUnmounted(() => {
  mounted = false
  unlistenClose?.()
  if (timer !== undefined) window.clearInterval(timer)
  window.removeEventListener('keydown', keydown)
  window.removeEventListener('hashchange', followWorkbenchLink)
  window.removeEventListener('beforeunload', beforeUnload)
})

function findProfile(id: string): Connection | undefined {
  return id === 'demo' ? demoConnection : connections.profiles.find(profile => profile.id === id)
}

function profileLabel(profile: Connection): string {
  const scope = [profile.settings?.tenant, profile.settings?.account].filter(Boolean)
  const tags = showTags.value ? profile.settings?.standardTags ?? [] : []
  return [profile.name, ...scope, ...tags].join(' · ')
}

function routeAllowed(candidate: WorkspaceRoute): boolean {
  const selected = findProfile(candidate.connectionId)
  return (!candidate.connectionId || !!selected) && workspaceAvailable(selected, candidate.workspace)
}

async function initialize(): Promise<void> {
  try {
    await connections.load()
  } catch (failure) {
    startError.value = errorMessage(failure)
  }
  const preferred = findProfile(startup.value)
  const fallback = defaultRoute(preferred ?? connections.active)
  navigation.initialize(fallback, routeAllowed)
  if (followWorkbenchLink()) {
    await jobs.load()
    return
  }
  if (new URLSearchParams(window.location.search).get('demo') === '1') openDemo()
  else if (preferred) navigation.go(fallback)
  await jobs.load()
}

function followWorkbenchLink(): boolean {
  const value = new URLSearchParams(window.location.hash.slice(1)).get('bucketWorkbench')
  if (!value) return false
  try {
    const candidate: unknown = JSON.parse(value)
    if (!isWorkspaceRoute(candidate) || !('workbenchBucket' in candidate) ||
      (candidate.workspace !== 'storageOps' && candidate.workspace !== 'cephAdmin') || !routeAllowed(candidate)) return false
    openRoute(candidate)
    return true
  } catch (failure) {
    startError.value = errorMessage(failure)
    return false
  }
}

function inspectWorkbenchBucket(key?: string): void {
  const current = workbenchRoute.value
  if (!current) return
  const destination = { workspace: current.source, page: 'buckets' as const }
  go(key ? { ...destination, workbenchBucket: key } : destination)
}

function synchronize(destination: WorkspaceRoute): void {
  const selected = findProfile(destination.connectionId)
  if (!routeAllowed(destination)) {
    navigation.replace(defaultRoute(selected ?? connections.active))
    return
  }
  if (destination.connectionId !== connections.activeId) connections.select(destination.connectionId)
  const allowed = workspaceAvailable(selected, 'browser') || workspaceAvailable(selected, 'manager')
  const key = allowed ? connections.contextKey : ''
  if (key !== browserContext) {
    creatingBucket.value = false
    browserContext = key
    void browser.changeConnection(allowed ? destination.connectionId : '', selected?.revision ?? '')
  }
  if (destination.workspace === 'browser') void browser.navigate(destination.bucket ?? '', destination.prefix ?? '')
}

function leavingBucket(action: () => void): void {
  if (bucketEditor.value?.busy) return
  if (bucketEditor.value?.dirty) departure.value = action
  else action()
}

function finishDeparture(): void {
  const action = departure.value
  departure.value = undefined
  bucketEditor.value?.discard()
  action?.()
}

function beforeUnload(event: BeforeUnloadEvent): void {
  if (!closing && bucketEditor.value?.dirty) event.preventDefault()
}

async function watchWindowClose(): Promise<void> {
  try {
    const windowHandle = getCurrentWindow()
    const unlisten = await windowHandle.onCloseRequested(event => {
      if (closing || !bucketEditor.value?.dirty && !bucketEditor.value?.busy) return
      event.preventDefault()
      leavingBucket(() => {
        closing = true
        void closeWindow()
      })
    })
    if (mounted) unlistenClose = unlisten
    else unlisten()
  } catch (failure) {
    if (mounted) startError.value = errorMessage(failure)
  }
}

async function closeWindow(): Promise<void> {
  try {
    await getCurrentWindow().close()
  } catch (failure) {
    closing = false
    startError.value = errorMessage(failure)
  }
}

function openRoute(destination: WorkspaceRoute): void {
  const current = route.value
  const sameBucket = current.workspace === 'manager' && current.page === 'buckets' &&
    destination.workspace === 'manager' && destination.page === 'buckets' &&
    current.connectionId === destination.connectionId && current.bucket === destination.bucket
  if (sameBucket) navigation.go(destination)
  else leavingBucket(() => navigation.go(destination))
}

function history(direction: 'back' | 'forward'): void {
  if (direction === 'back' ? navigation.canBack : navigation.canForward) {
    leavingBucket(() => direction === 'back' ? navigation.back() : navigation.forward())
  }
}

function keydown(event: KeyboardEvent): void {
  if (document.querySelector('dialog[open]')) return
  const typing = event.target instanceof HTMLElement && (event.target.matches('input, textarea, [contenteditable]'))
  if (event.altKey && !typing && (event.key === 'ArrowLeft' || event.key === 'ArrowRight')) {
    event.preventDefault()
    history(event.key === 'ArrowLeft' ? 'back' : 'forward')
    return
  }
  if (route.value.workspace !== 'browser' || !browser.bucket) return
  if (event.key === 'Escape') browserView.value?.closeInspector()
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
    event.preventDefault()
    browserView.value?.focusSearch()
  }
}

function go(destination: WorkspaceDestination): void {
  if (!workspaceAvailable(connections.active, destination.workspace)) return
  if (destination.workspace === 'browser' && !destination.bucket && browser.bucket) {
    destination = { workspace: 'browser', bucket: browser.bucket, prefix: browser.prefix }
  }
  openRoute({ ...destination, connectionId: connections.activeId })
}

function openMonitoringDestination(destination: WorkspaceDestination, connectionId = connections.activeId): void {
  const target = { ...destination, connectionId }
  if (routeAllowed(target)) openRoute(target)
}

function openWorkspaceRoot(): void {
  if (route.value.workspace === 'browser') openBucket('')
  else if (route.value.workspace === 'manager') go({ workspace: 'manager', page: 'overview' })
  else if (route.value.workspace === 'storageOps') go({ workspace: 'storageOps', page: 'overview' })
  else if (route.value.workspace === 'cephAdmin') go({ workspace: 'cephAdmin', page: 'overview' })
  else if (route.value.workspace === 'admin') go({ workspace: 'admin', page: 'overview' })
}

function openDemo(): void {
  openRoute(defaultRoute(demoConnection))
}

function selectContext(id: string): void {
  if (id !== connections.activeId) openRoute(defaultRoute(findProfile(id)))
}

function openProfile(profile: Connection): void {
  openRoute(defaultRoute(profile))
}

function openFavorite(location: Location): void {
  const destination: WorkspaceRoute = { workspace: 'browser', connectionId: location.connection, bucket: location.bucket, prefix: location.prefix }
  if (routeAllowed(destination)) openRoute(destination)
}

function openBucket(bucket: string, prefix?: string): void {
  if (!canBrowse.value) return
  openRoute({ workspace: 'browser', connectionId: connections.activeId, bucket: bucket || undefined, prefix: prefix || undefined })
}

function openEditor(value?: Connection): void {
  editedProfile.value = value
  editor.value = true
}

function closeEditor(): void {
  editor.value = false
  hasDraft.value = !!pausedSetup()
}

function connectionSaved(profile: Connection): void {
  closeEditor()
  openProfile(profile)
}

function bucketCreated(name: string): void {
  creatingBucket.value = false
  void browser.loadBuckets()
  if (route.value.workspace === 'manager') go({ workspace: 'manager', page: 'buckets', bucket: name })
  else openBucket(name)
}

function jobStarted(id: string): void {
  if (jobs.jobs.find(job => job.id === id)?.status !== 'running' && browser.bucket) void browser.loadRows()
}

function showJob(id: string): void {
  jobStarted(id)
  go({ workspace: 'admin', page: 'activity', jobId: id })
}
</script>

<template>
  <div class="app-shell" :class="{ 'sidebar-collapsed': !sidebar }" :data-window-chrome="windowChrome">
    <Sidebar v-if="sidebar" :route="route" :demo="demo" :has-draft="hasDraft" :drag-region="dragRegion"
      @add="openEditor()" @go="go" @bucket="openBucket" @create-bucket="creatingBucket = true" />
    <main class="app-main">
      <div class="shell-top">
        <header class="app-bar" :data-tauri-drag-region="dragRegion">
          <div class="shell-navigation">
            <button class="icon-button subtle" :aria-label="sidebar ? 'Hide sidebar' : 'Show sidebar'" :aria-expanded="sidebar" @click="sidebar = !sidebar">
              <PanelLeftClose v-if="sidebar" :size="18" /><PanelLeftOpen v-else :size="18" />
            </button>
            <div class="history-actions">
              <button class="icon-button subtle" aria-label="Go back" title="Go back (Alt + Left)" :disabled="!navigation.canBack" @click="history('back')"><ArrowLeft :size="17" /></button>
              <button class="icon-button subtle" aria-label="Go forward" title="Go forward (Alt + Right)" :disabled="!navigation.canForward" @click="history('forward')"><ArrowRight :size="17" /></button>
            </div>
          </div>
          <div class="shell-context">
            <AppSelect id="active-connection" :model-value="connections.activeId" :options="contextOptions"
              aria-label="Connection context" placeholder="Select a connection" @update:model-value="selectContext">
              <template #leading><Server :size="16" /></template>
            </AppSelect>
            <AppSelect v-if="realmOptions.length > 1" :model-value="connections.activeId" :options="realmOptions" aria-label="Same-realm profile" @update:model-value="selectContext" />
          </div>
          <div class="top-controls">
            <span v-if="demo" class="demo-label"><span class="status-dot online" /> Read-only demo</span>
            <span v-else-if="connections.active" class="connection-status">{{ providerNames[connections.active.provider] }}</span>
            <button class="icon-button subtle" type="button" :aria-label="`${workspaceNames[route.workspace]} help`" @click="help = true"><CircleHelp :size="18" /></button>
          </div>
          <WindowControls v-if="windowChrome === 'custom'" @error="startError = $event" />
        </header>
        <nav class="shell-breadcrumbs" aria-label="Workspace breadcrumb">
          <button v-if="resource || route.workspace === 'manager' || route.workspace === 'admin' || route.workspace === 'cephAdmin' || route.workspace === 'storageOps'"
            type="button" class="subtle" @click="openWorkspaceRoot">{{ workspaceNames[route.workspace] }}</button>
          <span v-else :aria-current="routeTitle(route) === workspaceNames[route.workspace] ? 'page' : undefined">{{ workspaceNames[route.workspace] }}</span>
          <template v-if="routeTitle(route) !== workspaceNames[route.workspace]">
            <ChevronRight :size="13" />
            <button v-if="resource && managerRoute?.page === 'buckets'" type="button" class="subtle" @click="go({ workspace: 'manager', page: 'buckets' })">{{ routeTitle(route) }}</button>
            <span v-else :aria-current="resource ? undefined : 'page'">{{ routeTitle(route) }}</span>
          </template>
          <template v-if="resource"><ChevronRight :size="13" /><span class="shell-resource" :title="resource" aria-current="location">{{ resource }}</span></template>
        </nav>
      </div>
      <Notice v-if="startError" tone="error">{{ startError }}</Notice>

      <div class="work-area">
        <FadeRegion>
          <KeepAlive include="BrowserWorkspace,ManagerWorkspace,MonitoringWorkspace,ActivityWorkspace,ToolsWorkspace,BucketWorkbench" :max="12">
            <BrowserWorkspace v-if="route.workspace === 'browser'" key="browser" ref="browserView" :demo="demo" :compact="compact" @add="openEditor()"
              @demo="openDemo" @create-bucket="creatingBucket = true" @tools="go({ workspace: 'manager', page: 'tools' })" @job="jobStarted" @navigate="openBucket" @favorite="openFavorite" />
            <MonitoringWorkspace v-else-if="monitoringRoute" :key="`monitoring:${monitoringRoute.workspace}:${monitoringRoute.pane}:${connections.contextKey}`"
              :workspace="monitoringRoute.workspace" :pane="monitoringRoute.pane" :metric="monitoringRoute.metric"
              :profile="connections.active" :profiles="connections.profiles" :compact="compact"
              @open="openMonitoringDestination" @create="creatingBucket = true" />
            <BucketWorkbench v-else-if="workbenchRoute" ref="workbenchView"
              :key="`workbench:${workbenchRoute.source}:${workbenchRoute.source === 'cephAdmin' ? connections.contextKey : demo}`"
              :source="workbenchRoute.source" :pane="workbenchRoute.pane" :target="workbenchRoute.target" :initial-bucket="workbenchRoute.bucket"
              :profiles="workbenchProfiles" :compact="compact" :link="workbenchLink"
              @overview="go({ workspace: 'storageOps', page: 'overview' })" @inventory="inspectWorkbenchBucket()"
              @inspect="inspectWorkbenchBucket" @connections="go({ workspace: 'admin', page: 'connections' })" />
            <ManagerWorkspace v-else-if="managerRoute?.page === 'buckets' && connections.active" ref="managerView"
              :key="`manager:${connections.contextKey}`" :profile="connections.active" :bucket="managerRoute.bucket" :notification-topic="managerRoute.notificationTopic"
              :demo="demo" :browse-available="canBrowse" :compact="compact" @inventory="go({ workspace: 'manager', page: 'buckets' })"
              @inspect="go({ workspace: 'manager', page: 'buckets', bucket: $event })" @browse="openBucket" @job="jobStarted"
              @tools="go({ workspace: 'manager', page: 'tools', bucket: $event })" @create="creatingBucket = true" @connections="go({ workspace: 'admin', page: 'connections' })" />
            <BucketRulesWorkspace v-else-if="managerRoute?.page === 'featureRules' && connections.active"
              :key="`rules:${connections.contextKey}`" :connection="connections.active.id"
              @inspect="go({ workspace: 'manager', page: 'buckets', bucket: $event })" />
            <AdminWorkspace v-else-if="route.workspace === 'cephAdmin' || managerRoute?.page === 'access' || (route.workspace === 'admin' && route.page === 'rgwIdentities')"
              :key="`access:${route.workspace}:${connections.contextKey}:${route.workspace === 'cephAdmin' ? route.group ?? '' : ''}:${route.workspace === 'cephAdmin' ? route.bucket ?? '' : ''}`"
              :scope="route.workspace === 'cephAdmin' ? 'ceph' : route.workspace === 'admin' ? 'managedRgw' : 's3'"
              :connection="connections.active" :bucket="route.workspace === 'cephAdmin' ? route.bucket ?? browser.bucket : browser.bucket"
              :initial-group="route.workspace === 'cephAdmin' ? route.group : undefined" :demo="demo" @open="openProfile"
              @notifications="(bucket, arn) => go({ workspace: 'manager', page: 'buckets', bucket, notificationTopic: arn })" />
            <ToolsWorkspace v-else-if="managerRoute?.page === 'tools'" :key="`tools:${connections.contextKey}`" :location="toolsLocation"
              :profiles="managerProfiles" :source-buckets="browser.buckets" :demo="demo" @browse="openFavorite" @job="showJob" @source="go({ workspace: 'manager', page: 'tools', bucket: $event })" />
            <ActivityWorkspace v-else-if="route.workspace === 'admin' && route.page === 'activity'" key="activity" :demo="demo" :preselected="route.jobId ?? null" :audit="route.audit"
              @browse="canBrowse ? go({ workspace: 'browser' }) : canManage ? go({ workspace: 'manager', page: 'buckets' }) : go({ workspace: 'admin', page: 'connections' })" />
            <WorkspacePreferences v-else-if="route.workspace === 'admin' && route.page === 'preferences'" v-model:compact="compact" v-model:show-tags="showTags"
              v-model:startup="startup" :profiles="connections.profiles" />
            <ConnectionsWorkspace v-else :has-draft="hasDraft" @add="openEditor()" @edit="openEditor" @browse="openProfile" @demo="openDemo" />
          </KeepAlive>
        </FadeRegion>
      </div>
      <footer class="app-status">
        <span><span class="status-dot" :class="{ online: demo || ((canBrowse || canManage) && !browser.bucketError && !browser.bucketLoading) }" />
          {{ demo ? 'Sample data' : connections.active?.endpoint || (connections.active ? 'AWS regional endpoint' : 'StoreForge desktop') }}</span>
        <button v-if="jobs.active && !demo" class="subtle" @click="go({ workspace: 'admin', page: 'activity' })"><Activity :size="14" /> {{ jobs.active }} running</button>
        <span v-else-if="connections.active">{{ connections.active.region }}<span class="separator">·</span>{{ connections.active.pathStyle ? 'Path style' : 'Virtual hosted' }}</span>
      </footer>
    </main>
  </div>
  <ConnectionEditor v-if="editor" :profile="editedProfile" :resume="!editedProfile && hasDraft" @close="closeEditor" @saved="connectionSaved" />
  <ConfirmDialog v-if="departure" title="Discard bucket drafts?"
    description="Your unsaved configuration or quota changes will be discarded when you leave this bucket."
    action="Discard drafts" danger @close="departure = undefined" @confirm="finishDeparture" />
  <WorkspaceHelp v-if="help" :workspace="route.workspace" @close="help = false" />
  <NewBucketDialog v-if="creatingBucket && connections.active && !demo" :profile="connections.active" @close="creatingBucket = false" @created="bucketCreated" />
</template>
