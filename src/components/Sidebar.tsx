import React, { useState, useEffect } from 'react';
import { Mic, Cpu, History, BookA, Settings, Sun, Moon, Monitor, RefreshCw, Download } from 'lucide-react';
import { toast } from 'sonner';
import type { Theme } from '../lib/theme';
import { isNative, backend, events, checkForAppUpdates, AppUpdateInfo } from '../lib/bridge';

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
  const [version, setVersion] = useState('0.3.24');
  // Downloadable update handle (resolved from the updater when one is pending).
  const [updateInfo, setUpdateInfo] = useState<AppUpdateInfo | null>(null);
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);

  // When the background watcher finds a release, resolve it so Install works in one click.
  useEffect(() => {
    if (!isNative) return;
    const load = (v: string | null) => {
      if (!v) { setUpdateInfo(null); return; }
      checkForAppUpdates(false).then(setUpdateInfo).catch(() => {});
    };
    backend.getPendingUpdate().then(load).catch(() => {});
    return events.onUpdateAvailable(load);
  }, []);

  const handleCheck = async (force = false) => {
    setChecking(true);
    try {
      const update = await checkForAppUpdates(force);
      setUpdateInfo(update);
      if (!force) backend.refreshUpdateStatus().catch(() => {});
      if (!update) toast.success(`Mumblr is up to date (v${version})`);
      else toast.info(force ? `Ready to reinstall v${update.version}` : `Version ${update.version} is available!`);
    } catch (e: any) {
      const msg = e?.message || String(e);
      if (msg.includes('404') || msg.includes('Not Found')) {
        toast.error('No releases published yet — push a v* tag to trigger a release build');
      } else {
        toast.error(`Could not check for updates: ${msg}`);
      }
    } finally {
      setChecking(false);
    }
  };

  const handleInstall = async () => {
    if (!updateInfo) return;
    setInstalling(true);
    try {
      toast.info('Downloading and installing update…');
      await updateInfo.downloadAndInstall();
    } catch {
      toast.error('Failed to install update');
      setInstalling(false);
    }
  };

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

      {/* Theme switcher + version + updates */}
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
          <div className="mt-2 text-center text-[10px] select-none tracking-wide text-faint">
            v{version}
          </div>
        )}
        {isNative && (
          <div className="mt-1.5 flex gap-1">
            {updateInfo ? (
              <button
                disabled={installing}
                onClick={handleInstall}
                className="flex-1 flex items-center justify-center gap-1.5 py-1.5 bg-accent text-accent-fg text-[11px] font-medium rounded-lg hover:bg-accent-strong transition-colors disabled:opacity-50"
              >
                <Download size={12} />
                {installing ? 'Installing…' : `Install v${updateInfo.version}`}
              </button>
            ) : (
              <>
                <button
                  disabled={checking}
                  onClick={() => handleCheck(false)}
                  className="flex-1 flex items-center justify-center gap-1.5 py-1.5 bg-surface-2 text-muted hover:text-fg text-[11px] font-medium rounded-lg transition-colors disabled:opacity-50"
                >
                  <RefreshCw size={12} className={checking ? 'animate-spin' : ''} />
                  {checking ? 'Checking…' : 'Check updates'}
                </button>
                <button
                  disabled={checking || installing}
                  onClick={() => handleCheck(true)}
                  title="Reinstall the latest build from server even if version numbers match"
                  className="px-2 py-1.5 bg-surface-2 text-muted hover:text-fg text-[11px] font-medium rounded-lg transition-colors disabled:opacity-50"
                >
                  Force
                </button>
              </>
            )}
          </div>
        )}
      </div>
    </aside>
  );
}
