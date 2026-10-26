import { useMutation, useQueries, useQuery, useQueryClient } from '@tanstack/react-query'
import { api, json } from './client'
import type {
  AppSettings,
  AppSettingsInput,
  CaptionPreview,
  ConnectionCheck,
  Character,
  CharacterInput,
  Fact,
  Job,
  Line,
  LineInput,
  LlmProvider,
  Portrait,
  Project,
  ProjectInput,
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
  facts: (characterId: number) => ['characters', characterId, 'facts'] as const,
  portraits: (characterId: number) => ['characters', characterId, 'portraits'] as const,
  scenes: (projectId: number) => ['projects', projectId, 'scenes'] as const,
  lines: (sceneId: number) => ['scenes', sceneId, 'lines'] as const,
  takes: (lineId: number) => ['lines', lineId, 'takes'] as const,
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

export function toCharacterInput(c: Character): CharacterInput {
  const { id: _id, project_id: _p, created_at: _c, ...rest } = c
  return rest
}

export const useFacts = (characterId: number) =>
  useQuery({ queryKey: keys.facts(characterId), queryFn: () => api<Fact[]>(`/characters/${characterId}/facts`) })

export function useAddFact(characterId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (text: string) => api<Fact>(`/characters/${characterId}/facts`, { method: 'POST', ...json({ text }) }),
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.facts(characterId) }),
  })
}

export function useDeleteFact(characterId: number) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => api<void>(`/facts/${id}`, { method: 'DELETE' }),
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.facts(characterId) }),
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

export function toSceneInput(s: Scene): SceneInput {
  const { id: _id, project_id: _p, ...rest } = s
  return rest
}

// --- lines ---

export const useLines = (sceneId: number | undefined) =>
  useQuery({
    queryKey: keys.lines(sceneId ?? 0),
    queryFn: () => api<Line[]>(`/scenes/${sceneId}/lines`),
    enabled: sceneId !== undefined,
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
