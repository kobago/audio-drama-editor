import { describe, expect, it } from 'vitest'
import type { Character } from '../api/types'
import { characterColors, checkLength, countChars, formatDuration, intensityLabel, performanceTags } from './script'

describe('countChars', () => {
  it('counts characters without whitespace', () => {
    expect(countChars('遅いよ、 もう。\n')).toBe(7)
  })

  it('counts surrogate pairs as one character', () => {
    expect(countChars('𠮷野家')).toBe(3)
  })
})

describe('checkLength', () => {
  it('returns null without a length class', () => {
    expect(checkLength('あ', null)).toBeNull()
  })

  it('checks the S/M/L ranges inclusively', () => {
    expect(checkLength('あいうえ', 'S')).toBe('short')
    expect(checkLength('あいうえお', 'S')).toBe('ok')
    expect(checkLength('あ'.repeat(15), 'S')).toBe('ok')
    expect(checkLength('あ'.repeat(16), 'S')).toBe('long')
    expect(checkLength('あ'.repeat(60), 'L')).toBe('ok')
  })
})

describe('formatDuration', () => {
  it('formats milliseconds as mm:ss.cc', () => {
    expect(formatDuration(8400)).toBe('00:08.40')
    expect(formatDuration(61_005)).toBe('01:01.01')
    expect(formatDuration(0)).toBe('00:00.00')
  })
})

describe('intensityLabel', () => {
  it('uses the same thresholds as the server', () => {
    expect(intensityLabel(undefined)).toBeNull()
    expect(intensityLabel(0.3)).toBe('弱')
    expect(intensityLabel(0.5)).toBe('中')
    expect(intensityLabel(0.67)).toBe('強')
  })
})

describe('performanceTags', () => {
  it('skips empty fields and labels the rest', () => {
    expect(performanceTags({ emotion: '不満', intensity: 0.2, distance: '近い', tempo: ' ', note: 'メモ' })).toEqual([
      { key: 'emotion', strong: '不満', label: '弱' },
      { key: 'distance', label: '距離 近い' },
    ])
  })
})

describe('characterColors', () => {
  const ch = (id: number, sort_order = 0) => ({ id, sort_order }) as Character

  it('assigns colors in creation order and caps at char-8', () => {
    const colors = characterColors([ch(3), ch(1), ...Array.from({ length: 8 }, (_, i) => ch(10 + i))])
    expect(colors.get(1)).toBe('var(--char-1)')
    expect(colors.get(3)).toBe('var(--char-2)')
    expect(colors.get(17)).toBe('var(--char-8)')
  })
})
