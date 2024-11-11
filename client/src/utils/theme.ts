export const isBrowserDark = () => window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches

const defaultTheme = isBrowserDark() ? 'dark' : 'light'
const whitelistedThemes = ['light', 'dark']

export function getSavedTheme() {
  const savedTheme = window.localStorage.getItem('theme')
  if (!savedTheme) return defaultTheme
  if (savedTheme === 'system') return defaultTheme
  if (!whitelistedThemes.includes(savedTheme)) return defaultTheme
  return savedTheme
}
