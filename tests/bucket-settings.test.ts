// @vitest-environment jsdom

import { describe, expect, it } from 'vitest'
import { configurations } from '../src/features/bucket/model/configuration'
import { newStatement, readPolicy } from '../src/features/bucket/model/policy-document'
import { addXmlField, readXml, setXmlValue, writeXml, xmlChildren, xmlValue } from '../src/features/bucket/model/xml-document'
import { xmlSettings } from '../src/features/bucket/model/xml-settings'

describe('bucket XML configuration', () => {
  it.each(configurations.filter(option => option.id !== 'policy'))('reads the $label example and empty draft', ({ id, example }) => {
    if (id === 'policy') throw new Error('Expected an XML setting')
    const { root } = xmlSettings[id]
    expect(readXml(example, root).documentElement.localName).toBe(root)
    expect(readXml('', root).documentElement.localName).toBe(root)
  })

  it('rejects malformed XML, other configuration types, and document types', () => {
    expect(() => readXml('<Tagging>', 'Tagging')).toThrow('valid XML')
    expect(() => readXml('<CORSConfiguration/>', 'Tagging')).toThrow('expects a Tagging')
    expect(() => readXml('<!DOCTYPE Tagging><Tagging/>', 'Tagging')).toThrow('document type')
  })

  it('escapes field values and preserves provider extensions and namespaces', () => {
    const parent = readXml('<s:VersioningConfiguration xmlns:s="http://s3.amazonaws.com/doc/2006-03-01/" xmlns:p="urn:provider"><p:Status detail="keep">Custom</p:Status></s:VersioningConfiguration>', 'VersioningConfiguration').documentElement
    setXmlValue(parent, { kind: 'text', name: 'Status', label: 'Status' }, 'R&D <team>')
    const source = writeXml(parent)
    expect(source).toContain('<s:Status>R&amp;D &lt;team&gt;</s:Status>')
    expect(source).toContain('<p:Status detail="keep">Custom</p:Status>')
    expect(xmlChildren(parent, 'Status')).toHaveLength(1)
  })

  it('keeps an empty value when creating or editing a tag', () => {
    const parent = readXml('<Tagging><TagSet/></Tagging>', 'Tagging').documentElement
    const tagSet = xmlChildren(parent, 'TagSet')[0]!
    const value = { kind: 'text', name: 'Value', label: 'Value', allowEmpty: true } as const
    addXmlField(tagSet, { kind: 'group', name: 'Tag', label: 'Tag', multiple: true, fields: [value] })
    const tag = xmlChildren(tagSet, 'Tag')[0]!
    expect(xmlChildren(tag, 'Value')).toHaveLength(1)
    setXmlValue(tag, value, 'production', xmlChildren(tag, 'Value')[0])
    setXmlValue(tag, value, '', xmlChildren(tag, 'Value')[0])
    expect(xmlChildren(tag, 'Value')[0]?.textContent).toBe('')
  })

  it('writes the ACL grantee type as a namespaced attribute', () => {
    const document = readXml('<AccessControlPolicy><Grantee/></AccessControlPolicy>', 'AccessControlPolicy')
    const grant = xmlChildren(document.documentElement, 'Grantee')[0]!
    const type = { kind: 'select', name: 'xsi:type', label: 'Grantee type', attribute: true } as const
    setXmlValue(grant, type, 'CanonicalUser')
    expect(writeXml(grant)).toContain('xsi:type="CanonicalUser"')
    expect(xmlValue(grant, type)).toBe('CanonicalUser')
    setXmlValue(grant, type, '')
    expect(xmlValue(grant, type)).toBe('')
  })
})

describe('bucket policy configuration', () => {
  it('starts with an empty statement list and independent statement drafts', () => {
    expect(readPolicy('')).toEqual({ Version: '2012-10-17', Statement: [] })
    expect(newStatement().Action).not.toBe(newStatement().Action)
  })

  it('retains a policy with a single statement and nested conditions', () => {
    const policy = { Statement: { Effect: 'Deny', Condition: { Bool: { 'aws:SecureTransport': 'false' } } } }
    expect(readPolicy(JSON.stringify(policy))).toEqual(policy)
  })

  it('rejects malformed JSON and non-object documents', () => {
    expect(() => readPolicy('{"Statement":')).toThrow('valid JSON')
    expect(() => readPolicy('[]')).toThrow('JSON object')
  })
})
