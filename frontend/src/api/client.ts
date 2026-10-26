export class ApiError extends Error {
  status: number

  constructor(status: number, message: string) {
    super(message)
    this.status = status
  }
}

export async function api<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`/api${path}`, {
    ...init,
    headers: init?.body instanceof FormData ? init.headers : { 'content-type': 'application/json', ...init?.headers },
  })
  if (!res.ok) {
    const body = await res.json().catch(() => ({}))
    throw new ApiError(res.status, body.error ?? res.statusText)
  }
  return res.status === 204 ? (undefined as T) : res.json()
}

export const json = (body: unknown): RequestInit => ({ body: JSON.stringify(body) })

/** `/media/` 配下の相対パスを URL にする。 */
export function fileUrl(path: string): string {
  return `/media/${path}`
}

/** ファイルをアップロードし、`/media/` 配下の相対パスを返す。 */
export async function uploadFile(file: File): Promise<string> {
  const form = new FormData()
  form.append('file', file)
  const res = await api<{ path: string }>('/files', { method: 'POST', body: form })
  return res.path
}
