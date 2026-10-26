import { createRootRoute, createRoute, createRouter, Link, Outlet } from '@tanstack/react-router'
import { CastPage } from './features/cast/CastPage'
import { CharacterPage, type CharacterTab } from './features/cast/CharacterPage'
import { ProjectLayout } from './features/layout/ProjectLayout'
import { ProjectsPage } from './features/projects/ProjectsPage'
import { ScriptEditorPage } from './features/script/ScriptEditorPage'
import { ProjectSettingsPage } from './features/settings/ProjectSettingsPage'

const rootRoute = createRootRoute({
  component: () => (
    <div className="app">
      <Outlet />
    </div>
  ),
  notFoundComponent: () => (
    <main className="page">
      <h1 className="page__title">ページが見つかりません</h1>
      <Link to="/">作品一覧へ戻る</Link>
    </main>
  ),
})

const projectsRoute = createRoute({ getParentRoute: () => rootRoute, path: '/', component: ProjectsPage })

const projectRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/projects/$projectId',
  params: {
    parse: (p) => ({ projectId: Number(p.projectId) }),
    stringify: (p) => ({ projectId: String(p.projectId) }),
  },
  component: ProjectLayout,
})

export const scriptRoute = createRoute({
  getParentRoute: () => projectRoute,
  path: '/',
  validateSearch: (s: Record<string, unknown>): { scene?: number } => ({
    scene: s.scene === undefined ? undefined : Number(s.scene),
  }),
  component: ScriptEditorPage,
})

export const castRoute = createRoute({ getParentRoute: () => projectRoute, path: 'cast', component: CastPage })

const TABS: CharacterTab[] = ['basic', 'speech', 'knowledge', 'voice']

export const characterRoute = createRoute({
  getParentRoute: () => projectRoute,
  path: 'cast/$characterId',
  params: {
    parse: (p) => ({ characterId: Number(p.characterId) }),
    stringify: (p) => ({ characterId: String(p.characterId) }),
  },
  validateSearch: (s: Record<string, unknown>): { tab?: CharacterTab } => ({
    tab: TABS.includes(s.tab as CharacterTab) ? (s.tab as CharacterTab) : undefined,
  }),
  component: CharacterPage,
})

export const settingsRoute = createRoute({
  getParentRoute: () => projectRoute,
  path: 'settings',
  component: ProjectSettingsPage,
})

const routeTree = rootRoute.addChildren([
  projectsRoute,
  projectRoute.addChildren([scriptRoute, castRoute, characterRoute, settingsRoute]),
])

export const router = createRouter({ routeTree })

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}
