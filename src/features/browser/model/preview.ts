import type { Preview } from './objects'

export const objectPreviewLimit = 50 * 1024 * 1024
export const textPreviewLimit = 64 * 1024

type StructuredPreview = { kind: 'csv'; rows: string[][] } | { kind: 'json'; formatted: string }

export interface TextPreview {
  raw: string
  structured: StructuredPreview | null
}

export function parseCsv(source: string): string[][] {
  if (!source) return []
  const rows: string[][] = []
  let row: string[] = []
  let field = ''
  let state: 'field' | 'quoted' | 'closed' = 'field'

  function finishField(): void {
    row.push(field)
    field = ''
    state = 'field'
  }

  function finishRow(): void {
    finishField()
    rows.push(row)
    row = []
  }

  for (let index = 0; index < source.length; index++) {
    const character = source.charAt(index)
    if (state === 'quoted') {
      if (character === '"' && source.charAt(index + 1) === '"') {
        field += '"'
        index++
      } else if (character === '"') {
        state = 'closed'
      } else {
        field += character
      }
    } else if (character === ',') {
      finishField()
    } else if (character === '\n' || character === '\r') {
      finishRow()
      if (character === '\r' && source.charAt(index + 1) === '\n') index++
    } else if (character === '"' && state === 'field' && !field) {
      state = 'quoted'
    } else {
      if (state === 'closed' || character === '"') throw new Error('Invalid CSV field.')
      field += character
    }
  }
  if (state === 'quoted') throw new Error('Unclosed CSV field.')
  if (row.length || field || state === 'closed') finishRow()
  return rows
}

export function previewText(contentType: string, text: string, truncated: boolean): TextPreview {
  const result: TextPreview = { raw: text, structured: null }
  if (truncated || !text) return result
  try {
    if (contentType === 'text/csv' || contentType === 'application/csv') {
      result.structured = { kind: 'csv', rows: parseCsv(text) }
    } else if (contentType === 'application/json' || contentType.endsWith('+json')) {
      result.structured = { kind: 'json', formatted: JSON.stringify(JSON.parse(text), null, 2) }
    }
  } catch {
    return result
  }
  return result
}

export function decodePreview(preview: Preview): Uint8Array<ArrayBuffer> {
  const limit = preview.kind === 'text' ? textPreviewLimit : objectPreviewLimit
  if (preview.data.length > 4 * Math.ceil(limit / 3)) throw new Error('The preview exceeds its size limit.')
  const data = Uint8Array.from(atob(preview.data), character => character.charCodeAt(0))
  if (data.length > limit) throw new Error('The preview exceeds its size limit.')
  return data
}

export function decodePreviewText(data: Uint8Array<ArrayBuffer>, truncated: boolean): string | undefined {
  try {
    const value = new TextDecoder('utf-8', { fatal: true }).decode(data, { stream: truncated })
    return /[\u0000-\u0008\u000b\u000c\u000e-\u001f]/.test(value) ? undefined : value
  } catch {
    return undefined
  }
}

export function searchPreview(text: string, phrase: string): { text: string; match: boolean }[] {
  if (!phrase || !text) return [{ text, match: false }]
  const escaped = phrase.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  const pattern = new RegExp(escaped, 'giu')
  const parts: { text: string; match: boolean }[] = []
  let offset = 0
  for (const match of text.matchAll(pattern)) {
    if (match.index > offset) parts.push({ text: text.slice(offset, match.index), match: false })
    parts.push({ text: match[0], match: true })
    offset = match.index + match[0].length
  }
  if (offset < text.length) parts.push({ text: text.slice(offset), match: false })
  return parts
}
