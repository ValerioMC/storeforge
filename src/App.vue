<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { Activity, Box, CircleHelp, FolderCog, List, PanelLeftClose, PanelLeftOpen, ShieldCheck, SlidersHorizontal } from 'lucide-vue-next'
import { errorMessage, isTauri } from './common/tauri'
import Sidebar from './components/shell/Sidebar.vue'
import WindowControls from './components/shell/WindowControls.vue'
import FadeRegion from './components/ui/FadeRegion.vue'
import Notice from './components/ui/Notice.vue'
import AdminWorkspace from './features/administration/components/AdminWorkspace.vue'
import BrowserWorkspace from './features/browser/components/BrowserWorkspace.vue'
import { useBrowser } from './features/browser/store/browser'
import NewBucketDialog from './features/bucket/components/NewBucketDialog.vue'
import ConnectionEditor from './features/connection/components/ConnectionEditor.vue'
import ConnectionsWorkspace from './features/connection/components/ConnectionsWorkspace.vue'
import type { Connection } from './features/connection/model/connection'
import { useConnections } from './features/connection/store/connections'
import ActivityWorkspace from './features/transfer/components/ActivityWorkspace.vue'
import ToolsWorkspace from './features/transfer/components/ToolsWorkspace.vue'
import { useJobs } from './features/transfer/store/jobs'

const connections = useConnections()
const browser = useBrowser()
const jobs = useJobs()
const activePage = ref<'Objects' | 'Tools' | 'Administration' | 'Activity' | 'Connections'>('Objects')
const pages = [
  { name: 'Objects', icon: Box }, { name: 'Tools', icon: FolderCog },
  { name: 'Administration', icon: ShieldCheck }, { name: 'Activity', icon: Activity },
] as const
const editor = ref(false)
const profile = ref<Connection>()
const creatingBucket = ref(false)
const focusJob = ref<string | null>(null)
const sidebar = ref(true)
const compact = ref(localStorage.getItem('storeforge.density') === 'compact')
const startError = ref('')
const browserView = ref<InstanceType<typeof BrowserWorkspace>>()
const demo = computed(() => connections.activeId === 'demo')
const windowChrome = (window as Window & { __storeforgeWindowChrome?: 'overlay' | 'custom' }).__storeforgeWindowChrome
const dragRegion = windowChrome ? 'deep' : undefined
let timer: number | undefined
let previousRunning = new Set<string>()

watch(() => connections.contextKey, () => {
  creatingBucket.value = false
  void browser.changeConnection(connections.browserContext?.connectionId ?? '')
}, { flush: 'sync' })
watch(compact, value => localStorage.setItem('storeforge.density', value ? 'compact' : 'comfortable'))
watch(() => jobs.jobs, jobList => {
  const completed = jobList.some(job => previousRunning.has(job.id) && job.status !== 'running')
  previousRunning = new Set(jobList.filter(job => job.status === 'running').map(job => job.id))
  if (completed && browser.bucket) void browser.loadRows()
})

onMounted(() => {
  void initialize()
  window.addEventListener('keydown', keydown)
  if (isTauri()) timer = window.setInterval(() => void jobs.load(), 1200)
})

onUnmounted(() => {
  if (timer !== undefined) window.clearInterval(timer)
  window.removeEventListener('keydown', keydown)
})

async function initialize(): Promise<void> {
  try {
    await connections.load()
    if (new URLSearchParams(window.location.search).get('demo') === '1') openDemo()
    await jobs.load()
  } catch (failure) {
    startError.value = errorMessage(failure)
  }
}

function keydown(event: KeyboardEvent): void {
  if (activePage.value !== 'Objects' || !browser.bucket || document.querySelector('dialog[open]')) return
  if (event.key === 'Escape') browserView.value?.closeInspector()
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
    event.preventDefault()
    browserView.value?.focusSearch()
  }
}

function openDemo(): void {
  connections.select('demo')
  activePage.value = 'Objects'
}

function openBucket(name: string): void {
  activePage.value = 'Objects'
  void browser.navigate(name)
}

function openEditor(value?: Connection): void {
  profile.value = value
  editor.value = true
}

function connectionSaved(): void {
  editor.value = false
  activePage.value = 'Objects'
}

async function bucketCreated(name: string): Promise<void> {
  creatingBucket.value = false
  await browser.loadBuckets()
  openBucket(name)
}

function jobStarted(id: string): void {
  focusJob.value = id
  if (jobs.jobs.find(job => job.id === id)?.status !== 'running' && browser.bucket) void browser.loadRows()
}

function showJob(id: string): void {
  jobStarted(id)
  activePage.value = 'Activity'
}
</script>

<template>
  <div class="app-shell" :class="{ 'sidebar-collapsed': !sidebar }" :data-window-chrome="windowChrome">
    <Sidebar v-if="sidebar" :demo="demo" :browse="activePage === 'Objects'" :drag-region="dragRegion" @add="openEditor()" @connections="activePage = 'Connections'"
      @bucket="openBucket" @create-bucket="creatingBucket = true" @demo="openDemo" />
    <main class="app-main">
      <header class="app-bar" :data-tauri-drag-region="dragRegion">
        <button class="icon-button subtle side-toggle" :aria-label="sidebar ? 'Hide sidebar' : 'Show sidebar'" :aria-expanded="sidebar" @click="sidebar = !sidebar">
          <PanelLeftClose v-if="sidebar" :size="18" /><PanelLeftOpen v-else :size="18" />
        </button>
        <nav class="app-nav" aria-label="Workspaces">
          <button v-for="page in pages" :key="page.name" :class="{ active: activePage === page.name }" :aria-current="activePage === page.name ? 'page' : undefined" @click="activePage = page.name">
            <component :is="page.icon" :size="17" /><span>{{ page.name }}</span>
            <span v-if="page.name === 'Activity' && jobs.active" class="activity-count">{{ jobs.active }}</span>
          </button>
        </nav>
        <div class="top-controls">
          <span v-if="demo" class="demo-label"><span class="status-dot online" /> Demo workspace</span>
          <span v-else class="connection-status"><span class="status-dot" :class="{ online: connections.active && !browser.bucketError && !browser.bucketLoading }" />{{ browser.bucketLoading ? 'Connecting…' : !connections.active ? 'No connection' : browser.bucketError ? 'Connection error' : 'Connected' }}</span>
          <details class="tool-menu preferences">
            <summary class="icon-button subtle" aria-label="View preferences" title="View preferences"><SlidersHorizontal :size="17" /></summary>
            <div class="pop-menu"><button :aria-pressed="compact" @click="compact = !compact"><List :size="16" /> {{ compact ? 'Comfortable rows' : 'Compact rows' }}</button>
              <button @click="openDemo"><CircleHelp :size="16" /> Explore the demo</button></div>
          </details>
        </div>
        <WindowControls v-if="windowChrome === 'custom'" @error="startError = $event" />
      </header>
      <Notice v-if="startError" tone="error">{{ startError }}</Notice>

      <div class="work-area">
        <FadeRegion>
          <KeepAlive include="BrowserWorkspace">
            <BrowserWorkspace v-if="activePage === 'Objects'" ref="browserView" :demo="demo" :compact="compact" @add="openEditor()" @demo="openDemo"
              @create-bucket="creatingBucket = true" @tools="activePage = 'Tools'" @job="jobStarted" />
            <ToolsWorkspace v-else-if="activePage === 'Tools'" :location="browser.location" :profiles="connections.profiles" :demo="demo" @job="showJob" />
            <AdminWorkspace v-else-if="activePage === 'Administration'" :connection="connections.active" :bucket="browser.bucket" :demo="demo" />
            <ActivityWorkspace v-else-if="activePage === 'Activity'" :demo="demo" :preselected="focusJob" @browse="activePage = 'Objects'" />
            <ConnectionsWorkspace v-else @add="openEditor()" @edit="openEditor" @browse="activePage = 'Objects'" />
          </KeepAlive>
        </FadeRegion>
      </div>
      <footer class="app-status">
        <span><span class="status-dot" :class="{ online: demo || (connections.active && !browser.bucketError && !browser.bucketLoading) }" />
          {{ demo ? 'Sample data · Read-only' : connections.active?.endpoint || (connections.active ? 'AWS regional endpoint' : 'StoreForge desktop') }}</span>
        <button v-if="jobs.active" class="subtle" @click="activePage = 'Activity'"><Activity :size="14" /> {{ jobs.active }} running</button>
        <span v-else-if="connections.active">{{ connections.active.region }}<span class="separator">·</span>{{ connections.active.pathStyle ? 'Path style' : 'Virtual hosted' }}</span>
      </footer>
    </main>
  </div>
  <ConnectionEditor v-if="editor" :profile="profile" @close="editor = false" @saved="connectionSaved" />
  <NewBucketDialog v-if="creatingBucket" :connection="browser.location.connection" @close="creatingBucket = false" @created="bucketCreated" />
</template>
