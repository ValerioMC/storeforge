export type UploadEncryption =
  | { kind: 's3' }
  | { kind: 'kms' | 'dualLayerKms'; keyId?: string }

export interface TransferAccess {
  customerKey?: string
  roleArn?: string
  encryption?: UploadEncryption
}

export type ManagedTransferMode = 'presigned' | 'sts' | 'proxy'
export type TransferOperation = 'read' | 'download' | 'upload'

export interface ManagedTransferPolicy {
  presigned: boolean
  sts: boolean
  proxy: boolean
  cors: 'enabled' | 'disabled' | 'unknown'
  exposesEtag: boolean
  stsAvailable: boolean
}

export interface ManagedTransferRequest<Result> {
  customerEncrypted?: boolean
  multipart?: boolean
  expiration?: string
  run: (mode: ManagedTransferMode) => Promise<Result>
}

export class ManagedTransferResponseError extends Error {
  constructor(readonly status: number) {
    super('The transfer endpoint returned HTTP ' + status + '.')
  }
}

export function allowedTransferModes(
  policy: ManagedTransferPolicy,
  operation: TransferOperation,
  customerEncrypted = false,
  multipart = false,
): ManagedTransferMode[] {
  const modes: ManagedTransferMode[] = []
  const direct = policy.cors !== 'disabled' && !(operation === 'upload' && multipart && !policy.exposesEtag)
  if (direct && policy.sts && policy.stsAvailable && !customerEncrypted) modes.push('sts')
  if (direct && policy.presigned) modes.push('presigned')
  if (policy.proxy) modes.push('proxy')
  return modes
}

export async function runManagedTransfer<Result>(
  policy: ManagedTransferPolicy,
  operation: TransferOperation,
  mode: ManagedTransferMode,
  request: ManagedTransferRequest<Result>,
): Promise<Result> {
  if (!allowedTransferModes(policy, operation, request.customerEncrypted, request.multipart).includes(mode)) {
    throw new Error('The selected transfer mode is unavailable for this operation.')
  }
  if (mode === 'sts' && !usableStsExpiration(request.expiration)) {
    throw new Error('Renew the temporary credentials before starting this operation.')
  }
  try {
    return await request.run(mode)
  } catch (error) {
    if (operation === 'upload' && ambiguousWrite(error)) {
      throw new Error('The upload outcome is unknown. Check the destination before starting another upload.')
    }
    throw error
  }
}

function usableStsExpiration(expiration: string | undefined): boolean {
  return !!expiration && Date.parse(expiration) - Date.now() > 2 * 60 * 1000
}

function ambiguousWrite(error: unknown): boolean {
  return error instanceof TypeError
    || (error instanceof DOMException && (error.name === 'AbortError' || error.name === 'TimeoutError'))
    || (error instanceof ManagedTransferResponseError && error.status >= 500)
}
