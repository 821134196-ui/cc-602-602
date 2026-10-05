let token = localStorage.getItem('token') || ''

export function getToken() {
  return token
}

export function setToken(t) {
  token = t || ''
  if (token) localStorage.setItem('token', token)
  else localStorage.removeItem('token')
}

export async function api(path, { method = 'GET', body } = {}) {
  const headers = {}
  if (token) headers['Authorization'] = 'Bearer ' + token
  let payload = body
  if (body && !(body instanceof FormData)) {
    headers['Content-Type'] = 'application/json'
    payload = JSON.stringify(body)
  }
  const res = await fetch(path, { method, headers, body: payload })
  if (!res.ok) {
    let msg = `请求失败 (${res.status})`
    try {
      const data = await res.json()
      if (data.error) msg = data.error
    } catch {}
    throw new Error(msg)
  }
  return res.json()
}

/** 带鉴权下载附件，失败时弹出后端返回的错误 */
export async function downloadFile(versionId, filename) {
  const res = await fetch(`/api/files/versions/${versionId}/download`, {
    headers: { Authorization: 'Bearer ' + token }
  })
  if (!res.ok) {
    let msg = `下载失败 (${res.status})`
    try {
      msg = (await res.json()).error || msg
    } catch {}
    throw new Error(msg)
  }
  const blob = await res.blob()
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename || 'download'
  a.click()
  URL.revokeObjectURL(url)
}
