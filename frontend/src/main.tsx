import '@fontsource/zen-kaku-gothic-new/400.css'
import '@fontsource/zen-kaku-gothic-new/500.css'
import '@fontsource/zen-kaku-gothic-new/700.css'
import '@fontsource/shippori-mincho/400.css'
import '@fontsource/shippori-mincho/500.css'
import '@fontsource/shippori-mincho/600.css'
import '@fontsource/ibm-plex-mono/500.css'
import './styles/tokens.css'
import './styles/components.css'
import './styles/app.css'

import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { RouterProvider } from '@tanstack/react-router'
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { JobsProvider } from './api/jobs'
import { router } from './router'

// テーマは OS の設定に合わせる (tokens.css は data-theme で切り替える)
const dark = window.matchMedia('(prefers-color-scheme: dark)')
const applyTheme = () => {
  document.documentElement.dataset.theme = dark.matches ? 'dark' : 'light'
}
applyTheme()
dark.addEventListener('change', applyTheme)

const queryClient = new QueryClient({
  defaultOptions: { queries: { staleTime: 5_000, refetchOnWindowFocus: false } },
})

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <JobsProvider>
        <RouterProvider router={router} />
      </JobsProvider>
    </QueryClientProvider>
  </StrictMode>,
)
