import { useCallback, useEffect, useState } from 'react';
import { isNative, backend } from './bridge';

export type Theme = 'light' | 'dark' | 'system';

const KEY = 'mumblr_theme';

function systemPrefersDark(): boolean {
  return window.matchMedia?.('(prefers-color-scheme: dark)').matches ?? false;
}

/** Apply the resolved theme by toggling `.dark` on <html>. */
export function applyTheme(theme: Theme): void {
  const dark = theme === 'dark' || (theme === 'system' && systemPrefersDark());
  document.documentElement.classList.toggle('dark', dark);
}

/** Read the persisted theme synchronously (light default). Used at boot. */
export function initialTheme(): Theme {
  return (localStorage.getItem(KEY) as Theme) || (localStorage.getItem('dictando_theme') as Theme) || 'light';
}

/**
 * Theme state hook. Persists to localStorage (always) and to native settings
 * (when running in Tauri), and reacts to OS theme changes in "system" mode.
 */
export function useTheme() {
  const [theme, setThemeState] = useState<Theme>(initialTheme);

  // Hydrate from the native backend (source of truth on desktop).
  useEffect(() => {
    if (!isNative) return;
    backend.getSettings().then((s) => { if (s.theme) setThemeState(s.theme); }).catch(() => {});
  }, []);

  useEffect(() => {
    applyTheme(theme);
    localStorage.setItem(KEY, theme);
    if (theme !== 'system') return;
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const onChange = () => applyTheme('system');
    mq.addEventListener('change', onChange);
    return () => mq.removeEventListener('change', onChange);
  }, [theme]);

  const setTheme = useCallback((t: Theme) => {
    setThemeState(t);
    if (isNative) backend.updateSettings({ theme: t } as any).catch(() => {});
  }, []);

  return { theme, setTheme };
}
