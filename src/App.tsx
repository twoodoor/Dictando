import React, { useState, useEffect } from 'react';
import { auth, db } from './firebase';
import { onAuthStateChanged, User } from 'firebase/auth';
import { doc, setDoc, getDoc, addDoc, collection, serverTimestamp } from 'firebase/firestore';
import { Mic, Settings, History } from 'lucide-react';
import { Toaster, toast } from 'sonner';
import { AnimatePresence } from 'motion/react';
import { LoginScreen } from './components/LoginScreen';
import { RecordView } from './components/RecordView';
import { HistoryView } from './components/HistoryView';
import { SettingsView } from './components/SettingsView';
import { Loader2 } from 'lucide-react';

export default function App() {
  return (
    <>
      <Toaster theme="dark" position="bottom-center" />
      <AppContent />
    </>
  );
}

function AppContent() {
  const [user, setUser] = useState<User | null>(null);
  const [isAuthReady, setIsAuthReady] = useState(false);
  const [activeTab, setActiveTab] = useState<'record' | 'history' | 'settings'>('record');

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
            shortcut: ['Space'],
            createdAt: serverTimestamp(),
          }).catch(() => {});
        }
      }
    });
    return unsub;
  }, []);

  // Listen for Python-side dictations (pywebview bridge)
  useEffect(() => {
    if (!user) return;
    const handler = async (e: Event) => {
      const { text, durationMs, engine } = (e as CustomEvent).detail;
      if (!text) return;
      await addDoc(collection(db, `users/${user.uid}/dictations`), {
        text, durationMs, engine: engine || 'unknown',
        timestamp: serverTimestamp(),
      }).catch(() => {});
      toast.success('Dictation saved');
    };
    window.addEventListener('newDictation', handler);
    return () => window.removeEventListener('newDictation', handler);
  }, [user]);

  // Recording state toasts from Python bridge
  useEffect(() => {
    const handler = (e: Event) => {
      const { isRecording } = (e as CustomEvent).detail;
      if (isRecording) toast.info('Recording...');
      else toast.success('Processing...');
    };
    window.addEventListener('recordingStateChanged', handler);
    return () => window.removeEventListener('recordingStateChanged', handler);
  }, []);

  if (!isAuthReady) return (
    <div className="min-h-screen bg-zinc-950 flex items-center justify-center">
      <Loader2 className="w-8 h-8 text-zinc-500 animate-spin" />
    </div>
  );

  if (!user) return <LoginScreen />;

  return (
    <div className="min-h-screen bg-zinc-950 text-zinc-100 flex items-center justify-center p-4">
      <div className="w-full max-w-sm bg-zinc-900/80 backdrop-blur-xl border border-zinc-800 rounded-2xl shadow-2xl flex flex-col h-[640px]">

        {/* Title bar */}
        <div className="h-11 border-b border-zinc-800/60 flex items-center justify-between px-4 shrink-0">
          <div className="flex items-center gap-1.5">
            <span className="w-3 h-3 rounded-full bg-red-500/30 border border-red-500/60" />
            <span className="w-3 h-3 rounded-full bg-yellow-500/30 border border-yellow-500/60" />
            <span className="w-3 h-3 rounded-full bg-green-500/30 border border-green-500/60" />
          </div>
          <span className="text-[11px] font-semibold tracking-widest text-zinc-400 uppercase">Dictando</span>
          <div className="w-14" />
        </div>

        {/* Content */}
        <div className="flex-1 overflow-hidden relative">
          <AnimatePresence mode="wait">
            {activeTab === 'record'   && <RecordView   key="r" user={user} />}
            {activeTab === 'history'  && <HistoryView  key="h" user={user} />}
            {activeTab === 'settings' && <SettingsView key="s" user={user} />}
          </AnimatePresence>
        </div>

        {/* Nav */}
        <div className="h-16 border-t border-zinc-800/60 flex items-center justify-around px-2 shrink-0">
          {([
            { id: 'record',   icon: <Mic size={20} />,     label: 'Dictate'  },
            { id: 'history',  icon: <History size={20} />,  label: 'History'  },
            { id: 'settings', icon: <Settings size={20} />, label: 'Settings' },
          ] as const).map(({ id, icon, label }) => (
            <button
              key={id}
              onClick={() => setActiveTab(id)}
              className={`flex flex-col items-center justify-center w-16 h-12 rounded-xl transition-all ${
                activeTab === id
                  ? 'text-blue-400 bg-blue-500/10'
                  : 'text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800/50'
              }`}
            >
              {icon}
              <span className="text-[10px] font-medium mt-1">{label}</span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
