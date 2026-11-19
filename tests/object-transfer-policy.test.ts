// @vitest-environment jsdom

import { describe, expect, it, vi } from 'vitest'
import { createApp, nextTick } from 'vue'
import TransferAccessModal from '../src/features/transfer/components/TransferAccessModal.vue'
import type { TransferAccess } from '../src/features/transfer/model/access'
import { allowedTransferModes, runManagedTransfer, type ManagedTransferPolicy } from '../src/features/transfer/model/access'

const policy: ManagedTransferPolicy = {
  presigned: true, sts: true, proxy: true,
  cors: 'enabled', exposesEtag: true, stsAvailable: true,
}

describe('object transfer policy', () => {
  it('uses proxy when CORS is unavailable and excludes managed STS for SSE-C', () => {
    expect(allowedTransferModes({ ...policy, cors: 'disabled' }, 'upload')).toEqual(['proxy'])
    expect(allowedTransferModes(policy, 'download', true)).toEqual(['presigned', 'proxy'])
  })

  it('requires a visible ETag for web multipart writes', () => {
    expect(allowedTransferModes({ ...policy, exposesEtag: false }, 'upload', false, true)).toEqual(['proxy'])
    expect(allowedTransferModes({ ...policy, exposesEtag: false }, 'upload')).toEqual(['sts', 'presigned', 'proxy'])
  })

  it('requests renewal on an expired STS grant before dispatch', async () => {
    const run = vi.fn()
    await expect(runManagedTransfer(policy, 'download', 'sts', { expiration: '2000-01-01T00:00:00Z', run }))
      .rejects.toThrow('Renew the temporary credentials')
    expect(run).not.toHaveBeenCalled()
  })

  it('reports one ambiguous write without replay or mode fallback', async () => {
    const run = vi.fn().mockRejectedValue(new TypeError('transport lost'))
    await expect(runManagedTransfer(policy, 'upload', 'presigned', { run })).rejects.toThrow('upload outcome is unknown')
    expect(run).toHaveBeenCalledTimes(1)
  })
})

describe('managed transport dispatch', () => {
  const modes = ['sts', 'presigned', 'proxy'] as const
  const operations = ['read', 'download', 'upload'] as const
  const cases = operations.flatMap(operation => modes.map(mode => ({ mode, operation })))

  it.each(cases)('dispatches $operation over $mode exactly once', async ({ mode, operation }) => {
    const payload = new Blob(['fixture'])
    const run = vi.fn().mockResolvedValue(payload)
    await expect(runManagedTransfer(policy, operation, mode, { expiration: '2099-01-01T00:00:00Z', run })).resolves.toBe(payload)
    expect(run).toHaveBeenCalledExactlyOnceWith(mode)
  })

  it('stops before dispatch if all transports are disabled', async () => {
    const run = vi.fn()
    await expect(runManagedTransfer({ ...policy, cors: 'disabled', proxy: false }, 'read', 'presigned', { run }))
      .rejects.toThrow('unavailable')
    expect(run).not.toHaveBeenCalled()
  })
})

it('clears the customer key field before sending the one-operation request', async () => {
  HTMLDialogElement.prototype.showModal = function () { this.open = true }
  const host = document.createElement('div')
  document.body.append(host)
  const key = 'AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA='
  const perform = vi.fn<(access: TransferAccess) => void>()
  const app = createApp(TransferAccessModal, { operation: 'read', target: ' a.csv ', perform })
  app.mount(host)
  try {
    const dialog = document.querySelector<HTMLDialogElement>('dialog')!
    dialog.querySelector<HTMLButtonElement>('[role="combobox"]')!.click()
    await nextTick()
    const option = [...dialog.querySelectorAll<HTMLElement>('[role="option"]')]
      .find(item => item.textContent?.includes('SSE-C'))!
    option.click()
    await nextTick()
    const field = dialog.querySelector<HTMLInputElement>('input[type="password"]')!
    field.value = key
    perform.mockImplementation(() => expect(field.value).toBe(''))
    dialog.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    expect(perform).toHaveBeenCalledExactlyOnceWith({ customerKey: key })
    expect(field.getAttribute('value')).toBeNull()
  } finally {
    app.unmount()
    host.remove()
  }
})
