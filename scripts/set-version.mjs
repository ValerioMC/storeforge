import { readFileSync, writeFileSync } from 'node:fs'

function updateJson(path, update) {
  const value = JSON.parse(readFileSync(path, 'utf8'))
  update(value)
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`)
}

function replaceVersion(path, pattern, version) {
  const value = readFileSync(path, 'utf8')
  if (!pattern.test(value)) throw new Error(`Missing version in ${path}`)
  writeFileSync(path, value.replace(pattern, (_match, start, end) => `${start}${version}${end}`))
}

function setVersion(version) {
  if (!/^(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)$/.test(version ?? '')) {
    throw new Error('Expected a release version in X.Y.Z form.')
  }
  updateJson('package.json', value => { value.version = version })
  updateJson('package-lock.json', value => { value.version = version; value.packages[''].version = version })
  updateJson('src-tauri/tauri.conf.json', value => { value.version = version })
  replaceVersion('src-tauri/Cargo.toml', /(^\[package\]\r?\nname = "storeforge"\r?\nversion\s*=\s*")[^"]+(")/m, version)
  replaceVersion('src-tauri/Cargo.lock', /(\[\[package\]\]\r?\nname = "storeforge"\r?\nversion = ")[^"]+(")/, version)
}

setVersion(process.argv[2])
