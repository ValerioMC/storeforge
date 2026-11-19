<script setup lang="ts">
import { ExternalLink } from 'lucide-vue-next'
import { accessRights, accessScopes, grantKinds, type AccessReviewRow, type AccessSource } from '../model/platform-governance'
import type { SharedAction } from '../../connection/model/shared-service'

defineProps<{ rows: AccessReviewRow[]; busy: boolean }>()
const emit = defineEmits<{ review: [action: SharedAction] }>()

function sourceLabel(source: AccessSource): string {
  if (source.kind !== 'group') return grantKinds[source.kind]
  return source.groupName ?? (source.groupId ? `Group ${source.groupId}` : 'Group')
}
</script>

<template>
  <div class="workspace-table">
    <table aria-label="Effective access">
      <thead><tr><th scope="col">Principal</th><th scope="col">Resource</th><th scope="col">Effective rights and sources</th><th scope="col">Actions</th></tr></thead>
      <tbody>
        <tr v-for="row in rows" :key="row.key">
          <td>
            <strong>{{ row.principal.fullName || row.principal.email }}</strong>
            <span v-if="row.principal.fullName" class="secondary-line">{{ row.principal.email }}</span>
            <span v-if="!row.principal.isActive" class="chip principal-state">Inactive</span>
          </td>
          <td>
            <strong>{{ row.target.name }}</strong>
            <span class="secondary-line">{{ accessScopes[row.scope] }}</span>
            <span v-if="row.target.identifier" class="secondary-line">{{ row.target.identifier }}</span>
          </td>
          <td class="grant-column">
            <ul class="grant-list">
              <li v-for="right in row.rights" :key="right.code">
                <strong>{{ accessRights[right.code] }}</strong>
                <span v-for="source in right.sources" :key="`${source.kind}:${source.groupId}`" class="grant-source">
                  <button v-if="source.kind === 'group' && source.groupId && source.groupName" type="button"
                    class="subtle" :disabled="busy" :aria-label="`Review group ${source.groupName}`"
                    @click="emit('review', { reviewGroup: { id: source.groupId, search: source.groupName } })">
                    {{ sourceLabel(source) }} <ExternalLink :size="12" />
                  </button>
                  <span v-else class="chip">{{ sourceLabel(source) }}</span>
                </span>
              </li>
            </ul>
          </td>
          <td><button type="button" :disabled="busy" @click="emit('review', { reviewUser: { id: row.principal.id, search: row.principal.email } })">
            <ExternalLink :size="14" /> Review user
          </button></td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<style scoped>
.workspace-table td { vertical-align: top; padding-block: 9px; }
.workspace-table strong { color: var(--ink); }
.principal-state { margin-inline-start: 5px; }
.secondary-line { display: block; margin-block: 2px; }
.grant-column { min-width: 205px; white-space: normal; }
.grant-list { display: grid; gap: 5px; margin: 0; padding: 0; list-style: none; }
.grant-list li { display: flex; align-items: center; flex-wrap: wrap; gap: 3px 6px; }
.grant-source button { padding: 0 2px; min-height: 23px; color: var(--accent-deep); font-size: inherit; }
</style>
