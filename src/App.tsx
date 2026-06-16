import React, { useState, useEffect } from 'react';
import { auth, db } from './firebase';
import { onAuthStateChanged, User } from 'firebase/auth';
import { doc, setDoc, getDoc, addDoc, collection, serverTimestamp } from 'firebase/firestore';
import { Toaster, toast } from 'sonner';
import { AnimatePresence } from 'motion/react';
import { Loader2 } from 'lucide-react';
import { LoginScreen } from './components/LoginScreen';
import { RecordView } from './components/RecordView';
import { HistoryView } from './components/HistoryView';
import { SettingsView } from './components/SettingsView';
import { ModelsView } from './components/ModelsView';
import { DictionaryView } from './components/DictionaryView';
import { Titlebar } from './components/Titlebar';
import { Sidebar, type Tab } from './components/Sidebar';
import { MacOnboarding } from './components/MacOnboarding';
import { isNative, events, backend } from './lib/bridge';
import { useTheme } from './lib/theme';

export default function App() {
  const [user, setUser] = useState<User | null>(null);
  const [isAuthReady, setIsAuthReady] = useState(false);
  const [activeTab, setActiveTab] = useState<Tab>('record');
  const [showMacOnboarding, setShowMacOnboarding] = useState(false);
  const { theme, setTheme } = useTheme();

  // macOS: show first-run onboarding until Accessibility is granted.
  useEffect(() => {
    if (!isNative) return;
    (async () => {
      const platform = await backend.getPlatform().catch(() => '');
      if (platform !== 'macos') return;
      const granted = await backend.accessibilityStatus().catch(() => true);
      if (!granted) setShowMacOnboarding(true);
    })();
  }, []);

  useEffect(() => {
    const unsub = onAuthStateChanged(auth, async (u) => {
      setUser(u);
      setIsAuthReady(true);
      if (u) {
        const ref = doc(db, 'users', u.uid);
        const snap = await getDoc(ref).catch(() => null);
        if (!snap?.exists()) {
          await setDoc(ref, {
            email: u.email,
            displayName: u.displayName || '',
            photoURL: u.photoURL || '',
            shortcut: ['ControlLeft', 'Space'],
            createdAt: serverTimestamp(),
          }).catch(() => {});
        }
      }
    });
    return unsub;
  }, []);

  // Native (Tauri) transcription events: toast + optional cloud sync if logged in.
  useEffect(() => {
    if (!isNative) return;
    const offState = events.onRecordingState((state) => {
      if (state === 'recording') toast.info('Recording…');
      else if (state === 'transcribing') toast.message('Transcribing…');
    });
    const offText = events.onTranscription(async (r) => {
      if (!r.text) return;
      toast.success('Pasted');
      if (user) {
        await addDoc(collection(db, `users/${user.uid}/dictations`), {
          text: r.text, durationMs: r.durationMs, engine: r.engine || 'local',
          timestamp: serverTimestamp(),
        }).catch(() => {});
      }
    });
    return () => { offState(); offText(); };
  }, [user]);

  const resolvedToast = theme === 'system' ? 'system' : theme;

  if (!isAuthReady && !isNative) {
    return (
      <div className="h-screen w-screen bg-app flex items-center justify-center">
        <Loader2 className="w-8 h-8 text-muted animate-spin" />
      </div>
    );
  }

  if (!user && !isNative) {
    return (
      <>
        <Toaster theme={resolvedToast} position="bottom-center" />
        <LoginScreen />
      </>
    );
  }

  return (
    <div className="h-screen w-screen flex flex-col bg-app text-fg overflow-hidden">
      <Toaster theme={resolvedToast} position="bottom-center" richColors />
      {showMacOnboarding && <MacOnboarding onDone={() => setShowMacOnboarding(false)} />}
      <Titlebar />
      <div className="flex-1 flex overflow-hidden">
        <Sidebar active={activeTab} onNavigate={setActiveTab} theme={theme} setTheme={setTheme} />
        <main className="flex-1 relative overflow-hidden bg-app">
          <AnimatePresence mode="wait">
            {activeTab === 'record'     && <RecordView     key="r" user={user} />}
            {activeTab === 'models'      && <ModelsView      key="m" />}
            {activeTab === 'history'     && <HistoryView     key="h" user={user} />}
            {activeTab === 'dictionary'  && <DictionaryView  key="d" />}
            {activeTab === 'settings'    && <SettingsView    key="s" user={user} />}
          </AnimatePresence>
        </main>
      </div>
    </div>
  );
}
