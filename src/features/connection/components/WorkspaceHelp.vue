<script setup lang="ts">
import Modal from '../../../components/ui/Modal.vue'
import { workspaceNames, type DesktopWorkspace } from '../model/workspace'

defineProps<{ workspace: DesktopWorkspace }>()
defineEmits<{ close: [] }>()
</script>

<template>
  <Modal :title="`${workspaceNames[workspace]} help`" @close="$emit('close')">
    <div class="modal-form">
      <p v-if="workspace === 'browser'">Select a bucket, open a folder, or filter the loaded objects. Selecting a row opens its inspector. Checkboxes select objects for batch actions.</p>
      <p v-else-if="workspace === 'manager'">Open a bucket from the inventory to inspect its details and configuration. Storage tools use the source bucket and prefix shown on the page.</p>
      <p v-else-if="workspace === 'storageOps'">Open a storage context or filter the combined bucket inventory. Selections apply to the connection shown on each row.</p>
      <p v-else-if="workspace === 'cephAdmin'">Accounts and users use Admin Ops credentials. Monitoring uses Supervision credentials. Bucket and multisite actions use the dedicated Ceph Admin identity.</p>
      <p v-else>Set up a profile to connect to S3 or Ceph RGW. Check credentials before applying a profile. Workspace visibility and service identities are configured per profile.</p>
      <dl class="setup-summary">
        <div><dt>Back / forward</dt><dd><kbd>Alt</kbd> + <kbd>←</kbd> / <kbd>→</kbd></dd></div>
        <div v-if="workspace === 'browser'"><dt>Filter objects</dt><dd><kbd>Ctrl</kbd> or <kbd>⌘</kbd> + <kbd>K</kbd></dd></div>
        <div><dt>Close help or a drawer</dt><dd><kbd>Esc</kbd></dd></div>
      </dl>
    </div>
  </Modal>
</template>
