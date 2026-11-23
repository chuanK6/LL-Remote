import { describe, expect, it } from 'vitest'
import { formatTime, normalizeError, patchStream, quotaLabel, validateStream } from '../../src/domain'
import { defaultStream, emptySnapshot } from '../../src/types'

describe('连接与设备偏好', () => {
  it('未知时长不会显示为零或根据到期日期推算', () => {
    const account = emptySnapshot().account
    expect(quotaLabel(account)).toBe('暂无法读取')
    expect(quotaLabel({ ...account, quotaMinutes: 0 })).toBe('可用时长不足')
    expect(quotaLabel({ ...account, quotaMinutes: 125 })).toBe('2 小时 5 分钟')
  })
  it('单改画质或鼠标不会改变其他参数', () => {
    const preferences = { ...defaultStream(), bitrateKbps: 42000, fps: 120 }
    const resolution = patchStream(preferences, { kind: 'resolution', width: 2560, height: 1440 })
    expect(resolution.bitrateKbps).toBe(42000); expect(resolution.fps).toBe(120)
    const mouse = patchStream(resolution, { kind: 'mouse', mode: 'game' })
    expect(mouse).toEqual({ ...resolution, mouseMode: 'game' })
    expect(preferences.width).toBe(1920)
  })
  it('不向用户显示可能包含密码的异常字符串', () => {
    const error = normalizeError('failed request password=private-value')
    expect(error.message).not.toContain('private-value')
    expect(error.retryable).toBe(false)
  })
  it('校验原内核允许的帧率并使用客户端时区显示日期', () => {
    expect(validateStream({ ...defaultStream(), fps: 1 })).toBeTruthy()
    expect(formatTime('2026-11-21T01:00:00Z')).toContain('09:00')
  })
})
