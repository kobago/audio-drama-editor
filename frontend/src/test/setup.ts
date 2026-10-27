import '@testing-library/jest-dom/vitest'
import { cleanup } from '@testing-library/react'
import { afterEach } from 'vitest'

afterEach(() => cleanup())

// jsdom は scrollIntoView を持たない (一覧の選択中の項目を見える位置に出すのに使う)
Element.prototype.scrollIntoView ??= () => {}
