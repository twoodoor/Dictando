import React, { useState, useEffect } from 'react';
import { Mic, Cpu, History, BookA, Settings, Sun, Moon, Monitor } from 'lucide-react';
import type { Theme } from '../lib/theme';
import { isNative } from '../lib/bridge';

export type Tab = 'record' | 'models' | 'history' | 'dictionary' | 'settings';

const NAV: { id: Tab; label: string; icon: React.ReactNode }[] = [
  { id: 'record',     label: 'Dictate',    icon: <Mic size={18} /> },
  { id: 'models',     label: 'Models',     icon: <Cpu size={18} /> },
  { id: 'history',    label: 'History',    icon: <History size={18} /> },
  { id: 'dictionary', label: 'Dictionary', icon: <BookA size={18} /> },
  { id: 'settings',   label: 'Settings',   icon: <Settings size={18} /> },
];

const THEMES: { id: Theme; icon: React.ReactNode; label: string }[] = [
  { id: 'light', icon: <Sun size={14} />, label: 'Light' },
  { id: 'dark', icon: <Moon size={14} />, label: 'Dark' },
  { id: 'system', icon: <Monitor size={14} />, label: 'System' },
];

export function Sidebar({
  active,
  onNavigate,
  theme,
  setTheme,
}: {
  active: Tab;
  onNavigate: (tab: Tab) => void;
  theme: Theme;
  setTheme: (t: Theme) => void;
}) {
  const [version, setVersion] = useState('');

  useEffect(() => {
    if (isNative) {
      import('@tauri-apps/api/app').then(({ getVersion }) =>
        getVersion().then(setVersion)
      ).catch(() => {});
    }
  }, []);

  return (
    <aside className="w-[224px] shrink-0 bg-app border-r border-line flex flex-col">
      <nav className="flex-1 px-3 pt-4 space-y-1">
        {NAV.map(({ id, label, icon }) => {
          const on = active === id;
          return (
            <button
              key={id}
              onClick={() => onNavigate(id)}
              className={`w-full flex items-center gap-3 px-3 py-2 rounded-xl text-sm font-medium transition-colors ${
                on
                  ? 'bg-accent-soft text-accent'
                  : 'text-muted hover:bg-surface-2 hover:text-fg'
              }`}
            >
              <span className={on ? 'text-accent' : ''}>{icon}</span>
              {label}
            </button>
          );
        })}
      </nav>

      {/* Theme switcher + version */}
      <div className="p-3 border-t border-line">
        <div className="flex items-center gap-1 bg-surface-2 rounded-xl p-1">
          {THEMES.map(({ id, icon, label }) => (
            <button
              key={id}
              onClick={() => setTheme(id)}
              title={label}
              className={`flex-1 flex items-center justify-center gap-1.5 py-1.5 rounded-lg text-[11px] font-medium transition-colors ${
                theme === id ? 'bg-surface text-fg shadow-sm' : 'text-muted hover:text-fg'
              }`}
            >
              {icon}
            </button>
          ))}
        </div>
        {version && (
          <div className="mt-2 text-center text-[10px] text-faint select-none tracking-wide">
            v{version}
          </div>
        )}
      </div>
    </aside>
  );
}
