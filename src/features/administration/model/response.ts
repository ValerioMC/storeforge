export interface ResponseField {
  path: string
  value: string
}

export const credentialResponse = /(?:SecretAccessKey|secret[_-]key|session[_-]?token|password)/i

export function responseFields(response: string): ResponseField[] {
  const input = response.trim()
  if (!input) return []
  if (input.startsWith('<')) return xmlFields(input)
  if (!input.startsWith('{') && !input.startsWith('[')) return []
  try {
    const fields: ResponseField[] = []
    const document: unknown = JSON.parse(input)
    readFields(document, '', fields, 0)
    return fields
  } catch {
    return []
  }
}

function xmlFields(input: string): ResponseField[] {
  const document = new DOMParser().parseFromString(input, 'application/xml')
  if (document.querySelector('parsererror')) return []
  const fields: ResponseField[] = []
  const stack: { element: Element; path: string }[] = [{ element: document.documentElement, path: '' }]
  while (stack.length && fields.length < 120) {
    const node = stack.pop()
    if (!node) break
    const path = node.path ? `${node.path} › ${node.element.localName}` : node.element.localName
    if (!node.element.children.length) {
      const value = node.element.textContent?.trim()
      if (value) fields.push({ path, value })
      continue
    }
    const children = Array.from(node.element.children, element => ({ element, path }))
    stack.push(...children.reverse())
  }
  return fields
}

function readFields(value: unknown, path: string, fields: ResponseField[], depth: number): void {
  if (fields.length >= 120 || depth > 20) return
  if (typeof value === 'object' && value !== null) {
    for (const [key, child] of Object.entries(value)) {
      readFields(child, path ? `${path} › ${key}` : key, fields, depth + 1)
      if (fields.length >= 120) break
    }
  } else if (value !== null) {
    fields.push({ path, value: String(value) })
  }
}
