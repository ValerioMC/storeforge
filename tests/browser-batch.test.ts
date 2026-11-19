import { describe, expect, it } from 'vitest'
import { bulkAttributes, emptyAttributeDraft, literalPairs, versionCsv } from '../src/features/transfer/model/batch'

describe('Browser batch operations', () => {
  it('preserves exact tag keys and values, including whitespace and equals signs', () => {
    expect(literalPairs(' key = value = here \nnext=')).toEqual([
      { key: ' key ', value: ' value = here ' }, { key: 'next', value: '' },
    ])
    expect(() => literalPairs('same=1\nsame=2')).toThrow()
  })

  it('exports exact version identifiers and quotes keys containing commas and quotes', () => {
    const csv = versionCsv([{ key: ' a,"b" ', versionId: 'v=1', latest: false, deleteMarker: true, size: null, modified: null, storageClass: null }])
    expect(csv).toContain('" a,""b"" ","v=1","false","true"')
  })
})

describe('bulk attribute validation', () => {
  it('serializes every enabled attribute and preserves cleared values', () => {
    const draft = emptyAttributeDraft()
    draft.metadataEnabled = true
    draft.headers.contentType = 'text/plain'
    draft.metadata = 'one= exact '
    draft.tagsEnabled = true
    draft.tags = ' tag = value '
    draft.storageClass = 'STANDARD_IA'
    draft.acl = 'private'
    draft.legalHold = 'off'
    draft.retentionEnabled = true
    draft.retentionMode = 'GOVERNANCE'
    draft.retainUntil = '2099-03-01T12:30:00.000Z'
    draft.bypassGovernance = true
    expect(bulkAttributes(draft)).toEqual({
      metadata: {
        contentType: 'text/plain', cacheControl: null, contentDisposition: null, contentEncoding: null,
        contentLanguage: null, expires: null, metadata: { one: ' exact ' },
      },
      tags: [{ key: ' tag ', value: ' value ' }],
      storageClass: 'STANDARD_IA', acl: 'private', legalHold: false,
      retention: { mode: 'GOVERNANCE', until: draft.retainUntil, bypassGovernance: true },
    })
  })

  it('rejects an empty update, duplicate tags and missing retention settings', () => {
    const draft = emptyAttributeDraft()
    expect(() => bulkAttributes(draft)).toThrow('Choose at least one attribute')
    draft.tagsEnabled = true
    draft.tags = 'same=a\nsame=b'
    expect(() => bulkAttributes(draft)).toThrow('distinct')
    draft.tags = ''
    draft.retentionEnabled = true
    expect(() => bulkAttributes(draft)).toThrow('Select a retention mode')
  })
})
