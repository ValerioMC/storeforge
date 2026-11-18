import { describe, expect, it } from 'vitest'
import { demoObjects, demoVersions } from '../src/features/browser/model/demo'
import { displayName, parentPrefix } from '../src/features/browser/model/objects'

describe('literal S3 paths', () => {
  it('keeps empty and space-only path segments', () => {
    expect(parentPrefix('logs//')).toBe('logs/')
    expect(parentPrefix('/ /')).toBe('/')
    expect(parentPrefix('/')).toBe('')
    expect(displayName('logs//', 'logs/', true)).toBe('/')
    expect(displayName('logs/ report.txt ', 'logs/')).toBe(' report.txt ')
  })
})

describe('read-only sample inventory', () => {
  it('groups prefixes in browse mode and lists only objects in recursive mode', () => {
    expect(demoObjects('mission-archive', 'telemetry/', false).map(row => row.key)).toEqual([
      'telemetry/pass-018/', 'telemetry/pass-017/',
    ])
    expect(demoObjects('mission-archive', 'telemetry/', true)).toHaveLength(10)
  })

  it('matches exact keys when looking up a single object’s versions', () => {
    expect(demoVersions('telemetry/pass-018/pass-018-summary.csv', true)).toHaveLength(2)
    expect(demoVersions('telemetry/pass-018/pass-018-summary', true)).toHaveLength(0)
  })
})
