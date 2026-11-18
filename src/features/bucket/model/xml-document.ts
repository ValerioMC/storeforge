import type { XmlField, XmlValueField } from './xml-settings'

const s3Namespace = 'http://s3.amazonaws.com/doc/2006-03-01/'
const namespaceDeclaration = 'http://www.w3.org/2000/xmlns/'
const schemaNamespace = 'http://www.w3.org/2001/XMLSchema-instance'

export function readXml(input: string, root: string): XMLDocument {
  const document = new DOMParser().parseFromString(input || `<${root} xmlns="${s3Namespace}"/>`, 'application/xml')
  if (document.doctype || document.getElementsByTagNameNS('http://www.mozilla.org/newlayout/xml/parsererror.xml', 'parsererror').length) {
    throw new Error('Choose a valid XML document without a document type declaration.')
  }
  if (document.documentElement.localName !== root) {
    throw new Error(`This setting expects a ${root} XML document.`)
  }
  return document
}

export function writeXml(element: Element): string {
  return new XMLSerializer().serializeToString(element.ownerDocument)
}

export function xmlChildren(parent: Element, name: string): Element[] {
  return Array.from(parent.children).filter(child => child.localName === name && child.namespaceURI === parent.namespaceURI)
}

function addXmlChild(parent: Element, name: string): Element {
  const qualifiedName = parent.prefix ? `${parent.prefix}:${name}` : name
  const element = parent.ownerDocument.createElementNS(parent.namespaceURI, qualifiedName)
  return parent.appendChild(element)
}

export function addXmlField(parent: Element, field: XmlField): void {
  const element = addXmlChild(parent, field.name)
  if (field.kind !== 'group') return
  for (const child of field.fields) {
    if (child.kind !== 'group' && child.allowEmpty) addXmlChild(element, child.name)
  }
}

export function xmlValue(parent: Element, field: XmlValueField, child?: Element): string {
  if (field.attribute) return parent.getAttributeNS(schemaNamespace, 'type') ?? ''
  return child?.textContent ?? ''
}

export function setXmlValue(parent: Element, field: XmlValueField, value: string, child?: Element): void {
  if (field.attribute) {
    if (value) {
      if (!parent.lookupNamespaceURI('xsi')) parent.setAttributeNS(namespaceDeclaration, 'xmlns:xsi', schemaNamespace)
      parent.setAttributeNS(schemaNamespace, field.name, value)
    } else parent.removeAttributeNS(schemaNamespace, 'type')
    return
  }
  if (!value && !field.multiple && !field.allowEmpty) {
    child?.remove()
    return
  }
  const element = child ?? addXmlChild(parent, field.name)
  element.textContent = value
}
