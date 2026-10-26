import { getRouteApi, Link, Outlet } from '@tanstack/react-router'
import { useProject } from '../../api/hooks'
import { AppBar } from '../../components/AppBar'

const route = getRouteApi('/projects/$projectId')

export function ProjectLayout() {
  const { projectId } = route.useParams()
  const project = useProject(projectId)
  const nav = { params: { projectId }, activeProps: { 'aria-current': 'page' as const } }
  return (
    <>
      <AppBar project={project.data?.title}>
        <nav aria-label="作品内の画面" className="appbar__nav">
          <Link to="/projects/$projectId" {...nav} activeOptions={{ exact: true, includeSearch: false }}>
            台本
          </Link>
          <Link to="/projects/$projectId/cast" {...nav}>
            キャスト
          </Link>
          <Link to="/projects/$projectId/settings" {...nav}>
            設定
          </Link>
        </nav>
      </AppBar>
      <Outlet />
    </>
  )
}
