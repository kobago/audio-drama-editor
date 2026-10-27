import { useMutation, useQueries, useQuery, useQueryClient } from '@tanstack/react-query'
import { api, json } from './client'
import type {
  AppSettings,
  AppSettingsInput,
  CaptionPreview,
  ConnectionCheck,
  Character,
  CharacterInput,
  Generated,
  Job,
  Line,
  LineInput,
  LineVersion,
  LlmProvider,
  Memory,
  MemoryInput,
  MemoryMark,
  Portrait,
  Project,
  ProjectInput,
  Recall,
  Scene,
  SceneInput,
  SynthesisParams,
  Take,
  TtsStatus,
} from './types'

export const keys = {
  projects: ['projects'] as const,
  project: (id: number) => ['projects', id] as const,
  characters: (projectId: number) => ['projects', projectId, 'characters'] as const,
  character: (id: number) => ['characters', id] as const,
  memories: (characterId: number) => ['characters', characterId, 'memories'] as const,
  recall: (lineId: number) => ['lines', lineId, 'recall'] as const,
  portraits: (characterId: number) => ['characters', characterId, 'portraits'] as const,
  scenes: (projectId: number) => ['projects', projectId, 'scenes'] as const,
  lines: (sceneId: number) => ['scenes', sceneId, 'lines'] as const,
  takes: (lineId: number) => ['lines', lineId, 'takes'] as const,
  versions: (lineId: number) => ['lines', lineId, 'versions'] as const,
  caption: (lineId: number) => ['lines', lineId, 'caption'] as const,
  ttsStatus: ['tts', 'status'] as const,
  settings: ['settings'] as const,
}

// --- projects ---

export const useProjects = () => useQuery({ queryKey: keys.projects, queryFn: () => api<Project[]>('/projects') })

export const useProject = (id: number) =>
  useQuery({ queryKey: keys.project(id), queryFn: () => api<Project>(`/projects/${id}`) })

export function useCreateProject() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: ProjectInput) => api<Project>('/projects', { method: 'POST', ...json(input) }),
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.projects }),
  })
}

export function useUpdateProject(id: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: ProjectInput) => api<Project>(`/projects/${id}`, { method: 'PUT', ...json(input) }),
    onSuccess: (p) => {
      qc.setQueryData(keys.project(id), p)
      qc.invalidateQueries({ queryKey: keys.projects, exact: true })
    },
  })
}

/** 作品を消す。キャスト・シーン・セリフ・テイク・記憶と、その画像・音声のファイルも消える */
export function useDeleteProject(id: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: () => api<void>(`/projects/${id}`, { method: 'DELETE' }),
    onSuccess: () => {
      qc.removeQueries({ queryKey: keys.project(id) })
      qc.invalidateQueries({ queryKey: keys.projects, exact: true })
    },
  })
}

// --- characters ---

export const useCharacters = (projectId: number) =>
  useQuery({
    queryKey: keys.characters(projectId),
    queryFn: () => api<Character[]>(`/projects/${projectId}/characters`),
  })

export const useCharacter = (id: number) =>
  useQuery({ queryKey: keys.character(id), queryFn: () => api<Character>(`/characters/${id}`) })

export function useCreateCharacter(projectId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: Partial<CharacterInput> & { name: string }) =>
      api<Character>(`/projects/${projectId}/characters`, { method: 'POST', ...json(input) }),
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.characters(projectId) }),
  })
}

export function useUpdateCharacter(character: Character) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: CharacterInput) =>
      api<Character>(`/characters/${character.id}`, { method: 'PUT', ...json(input) }),
    onSuccess: (c) => {
      qc.setQueryData(keys.character(c.id), c)
      qc.invalidateQueries({ queryKey: keys.characters(c.project_id) })
    },
  })
}

/** キャラクターを消す。このキャラクターのセリフは話者なしで残るので、台本も読み直す */
export function useDeleteCharacter(character: Character) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: () => api<void>(`/characters/${character.id}`, { method: 'DELETE' }),
    onSuccess: () => {
      qc.removeQueries({ queryKey: keys.character(character.id) })
      qc.invalidateQueries({ queryKey: keys.characters(character.project_id) })
      qc.invalidateQueries({ queryKey: ['scenes'] })
    },
  })
}

export function toCharacterInput(c: Character): CharacterInput {
  const { id: _id, project_id: _p, created_at: _c, ...rest } = c
  return rest
}

/** 記憶を変えたら、セリフごとの検索結果 (記憶の本文を含む) も読み直す */
function invalidateMemories(qc: ReturnType<typeof useQueryClient>, characterId: number) {
  qc.invalidateQueries({ queryKey: keys.memories(characterId) })
  qc.invalidateQueries({ predicate: (q) => q.queryKey[0] === 'lines' && q.queryKey[2] === 'recall' })
}

export const useMemories = (characterId: number) =>
  useQuery({
    queryKey: keys.memories(characterId),
    queryFn: () => api<Memory[]>(`/characters/${characterId}/memories`),
  })

export function useAddMemory(characterId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: MemoryInput) =>
      api<Memory>(`/characters/${characterId}/memories`, { method: 'POST', ...json(input) }),
    onSuccess: () => invalidateMemories(qc, characterId),
  })
}

export function useUpdateMemory(characterId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, input }: { id: number; input: MemoryInput }) =>
      api<Memory>(`/memories/${id}`, { method: 'PUT', ...json(input) }),
    onSuccess: () => invalidateMemories(qc, characterId),
  })
}

export function useDeleteMemory(characterId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => api<void>(`/memories/${id}`, { method: 'DELETE' }),
    onSuccess: () => invalidateMemories(qc, characterId),
  })
}

// --- セリフごとの記憶 ---

export const useRecall = (lineId: number) =>
  useQuery({ queryKey: keys.recall(lineId), queryFn: () => api<Recall>(`/lines/${lineId}/recall`) })

/** 記憶を探す (数秒かかる)。`intent` が空ならセリフの本文で探す */
export function useRunRecall(lineId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (intent: string) =>
      api<Recall>(`/lines/${lineId}/recall`, { method: 'POST', ...json({ intent: intent || null }) }),
    onSuccess: (r) => qc.setQueryData(keys.recall(lineId), r),
  })
}

export function useMarkMemory(lineId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ memoryId, mark }: { memoryId: number; mark: MemoryMark | null }) =>
      api<Recall>(`/lines/${lineId}/memory-marks/${memoryId}`, { method: 'PUT', ...json({ mark }) }),
    onSuccess: (r) => qc.setQueryData(keys.recall(lineId), r),
  })
}

export const usePortraits = (characterId: number) =>
  useQuery({
    queryKey: keys.portraits(characterId),
    queryFn: () => api<Portrait[]>(`/characters/${characterId}/portraits`),
  })

/** 作品の全キャラクターの立ち絵を character_id ごとにまとめる。 */
export function useProjectPortraits(characters: Character[] | undefined): Map<number, Portrait[]> {
  const results = useQueries({
    queries: (characters ?? []).map((c) => ({
      queryKey: keys.portraits(c.id),
      queryFn: () => api<Portrait[]>(`/characters/${c.id}/portraits`),
    })),
  })
  const map = new Map<number, Portrait[]>()
  characters?.forEach((c, i) => map.set(c.id, results[i]?.data ?? []))
  return map
}

export function useAddPortrait(characterId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: { label: string; path: string }) =>
      api<Portrait>(`/characters/${characterId}/portraits`, { method: 'POST', ...json(input) }),
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.portraits(characterId) }),
  })
}

export function useDeletePortrait(characterId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => api<void>(`/portraits/${id}`, { method: 'DELETE' }),
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.portraits(characterId) }),
  })
}

// --- scenes ---

export const useScenes = (projectId: number) =>
  useQuery({ queryKey: keys.scenes(projectId), queryFn: () => api<Scene[]>(`/projects/${projectId}/scenes`) })

export function useCreateScene(projectId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: Partial<SceneInput>) =>
      api<Scene>(`/projects/${projectId}/scenes`, { method: 'POST', ...json(input) }),
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.scenes(projectId) }),
  })
}

export function useUpdateScene(scene: Scene) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: SceneInput) => api<Scene>(`/scenes/${scene.id}`, { method: 'PUT', ...json(input) }),
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.scenes(scene.project_id) }),
  })
}

/** シーンを消す。セリフとテイク、背景画像と音声のファイルも消える */
export function useDeleteScene(scene: Scene) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: () => api<void>(`/scenes/${scene.id}`, { method: 'DELETE' }),
    onSuccess: () => {
      qc.removeQueries({ queryKey: keys.lines(scene.id) })
      qc.invalidateQueries({ queryKey: keys.scenes(scene.project_id) })
    },
  })
}

export function toSceneInput(s: Scene): SceneInput {
  const { id: _id, project_id: _p, ...rest } = s
  return rest
}

// --- lines ---

/** 生成中の行があるあいだの読み直し間隔 */
const GENERATING_POLL_MS = 1500

export const useLines = (sceneId: number | undefined) =>
  useQuery({
    queryKey: keys.lines(sceneId ?? 0),
    queryFn: () => api<Line[]>(`/scenes/${sceneId}/lines`),
    enabled: sceneId !== undefined,
    // LLM が裏で書いている行があれば、書き終わるまで読み直す
    refetchInterval: (q) => (q.state.data?.some((l) => l.gen_state === 'generating') ? GENERATING_POLL_MS : false),
  })

export function toLineInput(l: Line): LineInput {
  return {
    kind: l.kind,
    character_id: l.character_id,
    text: l.text,
    tts_text: l.tts_text,
    length_class: l.length_class,
    performance: l.performance,
    caption_override: l.caption_override,
    pause_before_ms: l.pause_before_ms,
    pause_after_ms: l.pause_after_ms,
    portrait_id: l.portrait_id,
    intent: l.intent,
  }
}

export function useCreateLine(sceneId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: Partial<LineInput>) =>
      api<Line>(`/scenes/${sceneId}/lines`, { method: 'POST', ...json(input) }),
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.lines(sceneId) }),
  })
}

/** 行の更新。一覧のキャッシュを先に書き換えて、入力の反応を待たせない。 */
export function useUpdateLine(sceneId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, input }: { id: number; input: LineInput }) =>
      api<Line>(`/lines/${id}`, { method: 'PUT', ...json(input) }),
    onMutate: ({ id, input }) => {
      qc.setQueryData<Line[]>(keys.lines(sceneId), (lines) =>
        lines?.map((l) => (l.id === id ? { ...l, ...input } : l)),
      )
    },
    onSuccess: (line) => {
      qc.setQueryData<Line[]>(keys.lines(sceneId), (lines) => lines?.map((l) => (l.id === line.id ? line : l)))
      qc.invalidateQueries({ queryKey: keys.caption(line.id) })
    },
    onError: () => qc.invalidateQueries({ queryKey: keys.lines(sceneId) }),
  })
}

export function useDeleteLine(sceneId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => api<void>(`/lines/${id}`, { method: 'DELETE' }),
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.lines(sceneId) }),
  })
}

export function useReorderLines(sceneId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (ids: number[]) => api<void>(`/scenes/${sceneId}/lines/order`, { method: 'PUT', ...json({ ids }) }),
    onMutate: (ids) => {
      qc.setQueryData<Line[]>(keys.lines(sceneId), (lines) => {
        if (!lines) return lines
        const byId = new Map(lines.map((l) => [l.id, l]))
        return ids.flatMap((id) => byId.get(id) ?? [])
      })
    },
    onSettled: () => qc.invalidateQueries({ queryKey: keys.lines(sceneId) }),
  })
}

export const useCaption = (lineId: number | undefined) =>
  useQuery({
    queryKey: keys.caption(lineId ?? 0),
    queryFn: () => api<CaptionPreview>(`/lines/${lineId}/caption`),
    enabled: lineId !== undefined,
  })

// --- takes / tts ---

export const useTakes = (lineId: number | undefined) =>
  useQuery({
    queryKey: keys.takes(lineId ?? 0),
    queryFn: () => api<Take[]>(`/lines/${lineId}/takes`),
    enabled: lineId !== undefined,
  })

export function useAdoptTake(sceneId: number, lineId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (takeId: number) => api<Take>(`/takes/${takeId}/adopt`, { method: 'PUT' }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: keys.takes(lineId) })
      qc.invalidateQueries({ queryKey: keys.lines(sceneId) })
    },
  })
}

export function useDeleteTake(sceneId: number, lineId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (takeId: number) => api<void>(`/takes/${takeId}`, { method: 'DELETE' }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: keys.takes(lineId) })
      qc.invalidateQueries({ queryKey: keys.lines(sceneId) })
    },
  })
}

export const useTtsStatus = () =>
  useQuery({ queryKey: keys.ttsStatus, queryFn: () => api<TtsStatus>('/tts/status'), refetchInterval: 15_000 })

export function useCreateJobs() {
  return useMutation({
    mutationFn: (input: { line_ids: number[]; seed?: number; params?: Partial<SynthesisParams> }) =>
      api<Job[]>('/tts/jobs', { method: 'POST', ...json(input) }),
  })
}

// --- app settings ---

export const useSettings = () => useQuery({ queryKey: keys.settings, queryFn: () => api<AppSettings>('/settings') })

export function useSaveSettings() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: AppSettingsInput) => api<void>('/settings', { method: 'PUT', ...json(input) }),
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.settings }),
  })
}

/** 空文字で保存したキーを消す。 */
export function useSaveApiKey() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (key: string) => api<void>('/settings/opencode-api-key', { method: 'PUT', ...json({ key }) }),
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.settings }),
  })
}

export function useCheckConnection() {
  return useMutation({
    mutationFn: (provider: LlmProvider) =>
      api<ConnectionCheck>('/settings/check', { method: 'POST', ...json({ provider }) }),
  })
}

// --- LLM による生成 ---

/** 生成で行が増えた・変わったら、一覧と記憶の検索結果を読み直す */
function invalidateScene(qc: ReturnType<typeof useQueryClient>, sceneId: number) {
  qc.invalidateQueries({ queryKey: keys.lines(sceneId) })
  qc.invalidateQueries({ predicate: (q) => q.queryKey[0] === 'lines' && q.queryKey[2] === 'recall' })
}

export interface GenerateInput {
  /** この行の後ろに入れる。null ならシーンの末尾 */
  after_line_id: number | null
  count: number
  direction: string
}

/** 続きを書く: 計画を立てて行を入れる (セリフは裏で 1 行ずつ書かれる) */
export function useGenerate(sceneId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: GenerateInput) =>
      api<Generated>(`/scenes/${sceneId}/generate`, { method: 'POST', ...json(input) }),
    onSuccess: () => invalidateScene(qc, sceneId),
  })
}

/** 同じ条件でまとめて作り直す (前に入れた行は消える) */
export function useRedoGeneration(sceneId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (generationId: number) =>
      api<Generated>(`/generations/${generationId}/redo`, { method: 'POST' }),
    onSuccess: () => invalidateScene(qc, sceneId),
  })
}

/** 1 行を書き直す。今の案は「前の案」に残る */
export function useRegenerateLine(sceneId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, intent, length_class }: { id: number; intent?: string; length_class?: string | null }) =>
      api<Line>(`/lines/${id}/regenerate`, { method: 'POST', ...json({ intent, length_class }) }),
    onSuccess: (line) => {
      qc.setQueryData<Line[]>(keys.lines(sceneId), (lines) => lines?.map((l) => (l.id === line.id ? line : l)))
      qc.invalidateQueries({ queryKey: keys.versions(line.id) })
      invalidateScene(qc, sceneId)
    },
  })
}

export const useLineVersions = (lineId: number, enabled: boolean) =>
  useQuery({
    queryKey: keys.versions(lineId),
    queryFn: () => api<LineVersion[]>(`/lines/${lineId}/versions`),
    enabled,
  })

/** 前の案に戻す (今の案と入れ替わる) */
export function useRestoreVersion(sceneId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (versionId: number) => api<Line>(`/line-versions/${versionId}/restore`, { method: 'POST' }),
    onSuccess: (line) => {
      qc.setQueryData<Line[]>(keys.lines(sceneId), (lines) => lines?.map((l) => (l.id === line.id ? line : l)))
      qc.invalidateQueries({ queryKey: keys.versions(line.id) })
      qc.invalidateQueries({ queryKey: keys.caption(line.id) })
    },
  })
}

/** 演技を付ける (既にあるセリフに演技だけ) */
export function useDirect(sceneId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (lineIds: number[]) =>
      api<Line[]>(`/scenes/${sceneId}/direct`, { method: 'POST', ...json({ line_ids: lineIds }) }),
    onSuccess: (updated) => {
      const byId = new Map(updated.map((l) => [l.id, l]))
      qc.setQueryData<Line[]>(keys.lines(sceneId), (lines) => lines?.map((l) => byId.get(l.id) ?? l))
      for (const l of updated) qc.invalidateQueries({ queryKey: keys.caption(l.id) })
    },
  })
}
