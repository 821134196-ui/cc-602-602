export async function api(path, { method = 'GET', body, form } = {}) {
  const opts = { method, headers: {} }
  if (body !== undefined) {
    opts.headers['Content-Type'] = 'application/json'
    opts.body = JSON.stringify(body)
  }
  if (form) opts.body = form
  const res = await fetch(path, opts)
  const text = await res.text()
  let data = null
  try { data = JSON.parse(text) } catch { /* 非 JSON 响应 */ }
  if (!res.ok) throw new Error(data?.error || `请求失败 (${res.status})`)
  return data
}

export const STATUS_LABELS = {
  draft: '草稿（待导师确认）',
  confirmed: '导师已确认（待匿名检查）',
  checking: '匿名检查中',
  frozen: '稿件已冻结（待分配评阅人）',
  in_review: '评阅中',
  completed: '评阅完成（待开放意见）',
  released: '意见已开放',
  failed: '匿名检查未通过',
  superseded: '已被新轮次替代',
}

export function statusLabel(s) {
  return STATUS_LABELS[s] || s
}

export function fmtTime(t) {
  return t ? new Date(t).toLocaleString('zh-CN') : '-'
}
