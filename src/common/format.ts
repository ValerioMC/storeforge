const dateFormat = new Intl.DateTimeFormat(undefined, { year: 'numeric', month: 'short', day: 'numeric' })
const clockFormat = new Intl.DateTimeFormat(undefined, { year: 'numeric', month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })

export function formatDate(date: string | null | undefined, time = false): string {
  if (!date) return '—'
  const parsed = new Date(date)
  return Number.isNaN(parsed.getTime()) ? date : (time ? clockFormat : dateFormat).format(parsed)
}

export function formatSize(bytes: number | null | undefined): string {
  if (bytes == null) return '—'
  if (bytes < 1000) return `${bytes} B`
  const units = ['KB', 'MB', 'GB', 'TB', 'PB']
  const exponent = Math.min(Math.floor(Math.log10(bytes) / 3), units.length)
  return `${(bytes / 1000 ** exponent).toLocaleString(undefined, { maximumFractionDigits: 1 })} ${units[exponent - 1]}`
}
