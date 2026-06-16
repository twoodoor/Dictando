import {StrictMode} from 'react';
import {createRoot} from 'react-dom/client';
import App from './App.tsx';
import { Overlay } from './components/Overlay.tsx';
import './index.css';
import { applyTheme, initialTheme } from './lib/theme';
import { isNative } from './lib/bridge';

const root = createRoot(document.getElementById('root')!);

async function boot() {
  // The overlay window is identified by its Tauri window label (most reliable),
  // with a `#overlay` hash fallback for non-native testing.
  let overlay = location.hash === '#overlay';
  if (isNative) {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      if (getCurrentWindow().label === 'overlay') overlay = true;
    } catch { /* not in Tauri */ }
  }

  if (overlay) {
    document.documentElement.style.background = 'transparent';
    document.body.style.background = 'transparent';
    root.render(<Overlay />);
  } else {
    applyTheme(initialTheme()); // before first paint, avoids a flash
    root.render(
      <StrictMode>
        <App />
      </StrictMode>,
    );
  }
}

boot();
