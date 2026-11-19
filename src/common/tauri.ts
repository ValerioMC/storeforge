import { invoke, isTauri } from '@tauri-apps/api/core'

export { isTauri }

export function desktop<T>(command: string, arguments_: Record<string, unknown> = {}): Promise<T> {
  if (!isTauri()) {
    return Promise.reject(new Error('Open the StoreForge desktop app to connect to live storage.'))
  }
  return invoke<T>(command, arguments_)
}

export function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message
  if (typeof error === 'string') return error
  if (typeof error === 'object' && error !== null && 'message' in error) return String(error.message)
  return 'The operation failed. Try again.'
}

export function isMissing(error: unknown): boolean {
  return typeof error === 'object' && error !== null && 'kind' in error && error.kind === 'missing'
}

export function isUnsupported(error: unknown): boolean {
  return typeof error === 'object' && error !== null && 'kind' in error && error.kind === 'unsupported'
}
