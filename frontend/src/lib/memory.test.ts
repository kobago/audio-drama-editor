import { describe, expect, it } from 'vitest'
import { kindLabel, memoryMeta, parsePeople, scorePercent } from './memory'

describe('memory', () => {
  it('splits people on Japanese and ASCII commas', () => {
    expect(parsePeople('ボブ、母, 先生，')).toEqual(['ボブ', '母', '先生'])
    expect(parsePeople('  ')).toEqual([])
  })

  it('joins period and people', () => {
    expect(memoryMeta({ period: '小学4年の夏', people: ['ボブ', '母'] })).toBe('小学4年の夏・ボブ、母')
    expect(memoryMeta({ period: '', people: ['ボブ'] })).toBe('ボブ')
    expect(memoryMeta({ period: '', people: [] })).toBe('')
  })

  it('labels kinds and scales scores', () => {
    expect(kindLabel('habit')).toBe('好み・習慣')
    expect(scorePercent(1.5)).toBe(75)
    expect(scorePercent(3)).toBe(100)
  })
})
