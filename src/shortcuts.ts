export interface Shortcut { ctrl: boolean; alt: boolean; shift: boolean; key: string }
export type ShortcutAction = 'mouse' | 'disconnect' | 'display'
export type Shortcuts = Record<ShortcutAction, Shortcut>

export const shortcutActions: ShortcutAction[] = ['mouse', 'disconnect', 'display']
export const defaultShortcuts = (): Shortcuts => ({
  mouse: { ctrl: true, alt: true, shift: true, key: 'Z' },
  disconnect: { ctrl: true, alt: true, shift: true, key: 'D' },
  display: { ctrl: true, alt: true, shift: true, key: 'L' },
})

export function readShortcut(event: KeyboardEvent): Shortcut | null {
  const key = event.shiftKey && /^Digit[0-9]$/.test(event.code) ? event.code.slice(-1) : event.key.toUpperCase()
  if (event.metaKey || event.isComposing || event.getModifierState('AltGraph') ||
    !(event.ctrlKey || event.altKey || event.shiftKey) || !/^(?:[A-Z0-9]|F(?:[1-9]|1[0-2]))$/.test(key)) return null
  return { ctrl: event.ctrlKey, alt: event.altKey, shift: event.shiftKey, key }
}

export function formatShortcut(shortcut: Shortcut): string {
  return [shortcut.ctrl && 'Ctrl', shortcut.alt && 'Alt', shortcut.shift && 'Shift', shortcut.key].filter(Boolean).join(' + ')
}
