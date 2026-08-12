import React from 'react';
import { Minus, Square, X, Mic } from 'lucide-react';
import { isNative } from '../lib/bridge';

async function currentWindow() {
  const { getCurrentWindow } = await import('@tauri-apps/api/window');
  return getCurrentWindow();
}

/**
 * Custom window title bar (native only). The bar itself is the drag region
 * (`data-tauri-drag-region`); the control buttons are children without the
 * attribute, so they stay clickable. Gives an identical look on Windows + Mac.
 */
export function Titlebar() {
  if (!isNative) return null;

  const minimize = async () => (await currentWindow()).minimize();
  const toggleMaximize = async () => (await currentWindow()).toggleMaximize();
  // "Close" minimizes to the taskbar instead of quitting, so the global
  // dictation shortcut keeps working in the background. (A tray "Quit" comes
  // with Phase 3b.)
  const close = async () => (await currentWindow()).minimize();

  const btn =
    'titlebar-no-drag w-11 h-full flex items-center justify-center text-muted hover:bg-surface-2 hover:text-fg transition-colors';

  return (
    <div
      data-tauri-drag-region
      className="titlebar-drag h-9 shrink-0 flex items-center justify-between bg-app border-b border-line select-none"
    >
      <div className="flex items-center gap-2 pl-3 pointer-events-none">
        <span className="w-4 h-4 rounded-md bg-accent flex items-center justify-center">
          <Mic size={11} className="text-accent-fg" />
        </span>
        <span className="text-[12px] font-semibold tracking-wide text-fg">Mumblr</span>
      </div>
      <div className="flex items-stretch h-full">
        <button onClick={minimize} className={btn} aria-label="Minimize"><Minus size={15} /></button>
        <button onClick={toggleMaximize} className={btn} aria-label="Maximize"><Square size={12} /></button>
        <button
          onClick={close}
          className="titlebar-no-drag w-11 h-full flex items-center justify-center text-muted hover:bg-red-500 hover:text-white transition-colors"
          aria-label="Close"
        >
          <X size={16} />
        </button>
      </div>
    </div>
  );
}
