import type { PortalKey } from './sharing'

export type ClientExport = 'cyberduck' | 'winscp' | 'generic'

export interface ClientConnection {
  endpoint: URL
  bucket: string
  accessId: string
  forcePathStyle: boolean
  region: string | null
}

export function clientConnection(endpoint: string | null, key: Pick<PortalKey, 'access_key_id' | 'bucket_name'>, forcePathStyle: boolean, region: string | null): ClientConnection | undefined {
  if (!endpoint || !key.bucket_name || !key.access_key_id) return undefined
  const url = parseEndpoint(endpoint)
  if (!url || !['https:', 'http:'].includes(url.protocol) || url.username || url.password || url.search || url.hash) return undefined
  if (/[\r\n]/.test(key.access_key_id) || !/^[a-z0-9][a-z0-9.-]*[a-z0-9]$/.test(key.bucket_name)) return undefined
  return { endpoint: url, bucket: key.bucket_name, accessId: key.access_key_id, forcePathStyle, region }
}

export function clientDocument(format: ClientExport, connection: ClientConnection): string {
  const secure = connection.endpoint.protocol === 'https:'
  const port = connection.endpoint.port || (secure ? '443' : '80')
  const { hostname } = connection.endpoint
  switch (format) {
    case 'cyberduck': return cyberduck(connection, hostname, port)
    case 'winscp': return [
      `[Sessions\\StoreForge-${connection.bucket}]`,
      `HostName=${hostname}`, `PortNumber=${port}`, 'FSProtocol=7', `Ftps=${secure ? 1 : 0}`,
      `UserName=${connection.accessId}`, `RemoteDirectory=/${connection.bucket}`, `S3UrlStyle=${connection.forcePathStyle ? 1 : 0}`, '',
    ].join('\n')
    case 'generic': return [
      'S3 client connection', `Endpoint: ${connection.endpoint.href}`, `Bucket: ${connection.bucket}`,
      `Access ID: ${connection.accessId}`, `Addressing: ${connection.forcePathStyle ? 'path style' : 'virtual hosted'}`,
      ...(connection.region ? [`Region: ${connection.region}`] : []), 'Enter the secret when the client prompts for the password.', '',
    ].join('\n')
  }
}

function parseEndpoint(value: string): URL | undefined {
  try {
    return new URL(value)
  } catch {
    return undefined
  }
}

function cyberduck(connection: ClientConnection, hostname: string, port: string): string {
  const entries = [
    ['Protocol', 's3'], ['Nickname', `StoreForge · ${connection.bucket}`], ['Hostname', hostname],
    ['Port', port], ['Username', connection.accessId], ['Path', `/${connection.bucket}`],
  ].map(([key, value]) => `  <key>${key}</key>\n  <string>${escapeXml(value ?? '')}</string>`)
  if (connection.forcePathStyle) entries.push('  <key>Custom</key>\n  <dict><key>s3.bucket.virtualhost.disable</key><string>true</string></dict>')
  return ['<?xml version="1.0" encoding="UTF-8"?>', '<plist version="1.0">', '<dict>', ...entries, '</dict>', '</plist>', ''].join('\n')
}

function escapeXml(value: string): string {
  const entities: Record<string, string> = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&apos;' }
  return value.replace(/[&<>"']/g, character => entities[character] ?? character)
}
