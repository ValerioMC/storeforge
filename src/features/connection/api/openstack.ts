import { desktop, isTauri } from '../../../common/tauri'
import type { Connection } from '../model/connection'
import type { OpenStackActivation, OpenStackAuthentication, OpenStackCloud, OpenStackCloudSettings,
  OpenStackCloudStatus, OpenStackProjectAccess, OpenStackSession } from '../model/openstack'

export const openStackApi = {
  list: () => isTauri() ? desktop<OpenStackCloudStatus[]>('list_openstack_clouds') : Promise.resolve([]),
  save: (id: string | null, settings: OpenStackCloudSettings) =>
    desktop<OpenStackCloud>('save_openstack_cloud', { id, settings }),
  disconnect: (id: string, forget: boolean) => desktop<void>('disconnect_openstack_cloud', { id, forget }),
  authenticate: (id: string, authentication: OpenStackAuthentication) =>
    desktop<OpenStackSession>('authenticate_openstack', { id, authentication }),
  discover: (id: string, projectId: string) =>
    desktop<OpenStackProjectAccess>('discover_openstack_project', { id, projectId }),
  activate: (id: string, selection: OpenStackActivation) =>
    desktop<Connection>('activate_openstack_project', { id, selection }),
}
