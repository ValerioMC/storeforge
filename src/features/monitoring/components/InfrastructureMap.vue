<script setup lang="ts">
import { computed } from 'vue'
import { MapPin } from 'lucide-vue-next'
import world from '../../../assets/monitoring-world.png'
import WorkspacePanel from '../../../components/shell/WorkspacePanel.vue'
import { validCoordinates, type Connection } from '../../connection/model/connection'
import { latestHealth, type EndpointReading } from '../model/dashboard'

const props = defineProps<{ endpoints: EndpointReading[]; profiles: Connection[] }>()
defineEmits<{ open: [connection: string] }>()

const markers = computed(() => props.endpoints.flatMap(endpoint => {
  const match = props.profiles.find(profile => (profile.endpointId ?? profile.id) === (endpoint.profile.endpointId ?? endpoint.profile.id) &&
    validCoordinates(profile.settings?.coordinates?.latitude, profile.settings?.coordinates?.longitude))
  const point = match?.settings?.coordinates
  if (!point) return []
  return [{
    id: endpoint.profile.id,
    name: endpoint.profile.name,
    status: latestHealth(endpoint.view)?.status,
    position: { '--longitude': `${(point.longitude + 180) / 3.6}%`, '--latitude': `${(90 - point.latitude) / 1.8}%` },
  }]
}))
</script>

<template>
  <WorkspacePanel title="Infrastructure map" :description="`${markers.length} of ${endpoints.length} endpoints with coordinates`">
    <div class="monitoring-map">
      <img :src="world" alt="" width="656" height="328" />
      <button v-for="marker in markers" :key="marker.id" type="button" :style="marker.position" :data-state="marker.status"
        :aria-label="`Open endpoint ${marker.name}`" :title="marker.name" @click="$emit('open', marker.id)"><MapPin :size="17" /></button>
      <p v-if="!markers.length">Add endpoint coordinates in Connections.</p>
    </div>
  </WorkspacePanel>
</template>
