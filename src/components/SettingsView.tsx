import React, { useState, useEffect } from 'react';
import { auth, db } from '../firebase';
import { signOut, User } from 'firebase/auth';
import { doc, setDoc, getDoc } from 'firebase/firestore';
import { Settings, LogOut } from 'lucide-react';
import { toast } from 'sonner';
import { motion } from 'motion/react';

const LANGUAGES = [
  'Auto-detect','English','Spanish','French','German','Italian',
  'Portuguese','Romanian','Dutch','Russian','Chinese','Japanese',
  'Korean','Arabic','Hindi','Turkish','Polish','Ukrainian',
];

function Toggle({ checked, onChange }: { checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className="relative inline-flex items-center cursor-pointer">
      <input type="checkbox" className="sr-only peer" checked={checked} onChange={e => onChange(e.target.checked)} />
      <div className="w-10 h-5 bg-zinc-700 rounded-full peer peer-checked:bg-blue-500 after:content-[''] after:absolute after:top-0.5 after:left-0.5 after:bg-white after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:after:translate-x-5" />
    </label>
  );
}

export function SettingsView({ user }: { user: User }) {
  const [shortcut, setShortcut] = useState<string[]>(['Space']);
  const [language, setLanguage] = useState('Auto-detect');
  const [autoCopy, setAutoCopy] = useState(true);
  const [autoClear, setAutoClear] = useState(false);
  const [apiKey, setApiKey] = useState('');
  const [audioDevices, setAudioDevices] = useState<MediaDeviceInfo[]>([]);
  const [microphoneId, setMicrophoneId] = useState('default');
  const [isRecordingShortcut, setIsRecordingShortcut] = useState(false);
  const [tempKeys, setTempKeys] = useState<Set<string>>(new Set());

  // Load from Firestore
  useEffect(() => {
    getDoc(doc(db, 'users', user.uid)).then(snap => {
      if (!snap.exists()) return;
      const d = snap.data();
      if (d.shortcut) setShortcut(Array.isArray(d.shortcut) ? d.shortcut : [d.shortcut]);
      if (d.language) setLanguage(d.language);
      if (d.autoCopy !== undefined) setAutoCopy(d.autoCopy);
      if (d.autoClear !== undefined) setAutoClear(d.autoClear);
      if (d.apiKey) setApiKey(d.apiKey);
      if (d.microphoneId) setMicrophoneId(d.microphoneId);
    }).catch(() => {});
  }, [user.uid]);

  // Enumerate mic devices
  useEffect(() => {
    navigator.mediaDevices.getUserMedia({ audio: true })
      .then(s => { s.getTracks().forEach(t => t.stop()); return navigator.mediaDevices.enumerateDevices(); })
      .then(devs => setAudioDevices(devs.filter(d => d.kind === 'audioinput')))
      .catch(() => {});
  }, []);

  const save = async (key: string, value: any) => {
    await setDoc(doc(db, 'users', user.uid), { [key]: value }, { merge: true }).catch(() => {});
    // Sync to Python backend if running in pywebview
    const pyApi = (window as any).pywebview?.api;
    if (pyApi) {
      try { pyApi.sync_settings(JSON.stringify({ [key]: value })); } catch { /* */ }
    }
    toast.success('Saved');
  };

  // Shortcut capture
  useEffect(() => {
    if (!isRecordingShortcut) return;
    const down = (e: KeyboardEvent) => { e.preventDefault(); setTempKeys(p => new Set(p).add(e.code)); };
    const up = async (e: KeyboardEvent) => {
      e.preventDefault();
      if (tempKeys.size > 0) {
        const sc = Array.from(tempKeys);
        setShortcut(sc);
        setIsRecordingShortcut(false);
        setTempKeys(new Set());
        await save('shortcut', sc);
      }
    };
    window.addEventListener('keydown', down);
    window.addEventListener('keyup', up);
    return () => { window.removeEventListener('keydown', down); window.removeEventListener('keyup', up); };
  }, [isRecordingShortcut, tempKeys]);

  const formatKey = (code: string) =>
    code.replace('Key','').replace('Digit','').replace('Left','').replace('Right','').replace('Space','Space');

  const isPywebview = !!(window as any).pywebview;

  return (
    <motion.div
      initial={{ opacity: 0, x: 16 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -16 }}
      className="absolute inset-0 overflow-y-auto p-4"
    >
      {/* Profile */}
      <div className="flex items-center gap-3 mb-5 px-1">
        <img src={user.photoURL || ''} alt="" className="w-10 h-10 rounded-full border border-zinc-700" />
        <div>
          <div className="text-sm font-medium text-zinc-100">{user.displayName}</div>
          <div className="text-xs text-zinc-500">{user.email}</div>
        </div>
      </div>

      <div className="space-y-3 pb-4">
        {/* Shortcut */}
        <section className="bg-zinc-800/50 border border-zinc-700/40 rounded-2xl p-4">
          <div className="flex items-center justify-between mb-3">
            <div>
              <h3 className="text-sm font-medium text-zinc-100">Hotkey</h3>
              <p className="text-xs text-zinc-500 mt-0.5">Hold to record</p>
            </div>
            <Settings className="w-4 h-4 text-zinc-600" />
          </div>
          <div className="flex flex-wrap gap-1.5 mb-3 min-h-[28px]">
            {(isRecordingShortcut ? Array.from(tempKeys) : shortcut).map(k => (
              <kbd key={k} className="px-2.5 py-1 bg-zinc-700 text-zinc-100 rounded-lg text-xs font-mono border border-zinc-600">
                {formatKey(k)}
              </kbd>
            ))}
            {isRecordingShortcut && tempKeys.size === 0 && (
              <span className="text-xs text-zinc-500 animate-pulse">Press keys...</span>
            )}
          </div>
          <button
            onClick={() => { setIsRecordingShortcut(true); setTempKeys(new Set()); }}
            className={`w-full py-2 rounded-xl text-sm font-medium transition-all ${
              isRecordingShortcut
                ? 'bg-blue-500 text-white shadow-[0_0_16px_rgba(59,130,246,0.3)]'
                : 'bg-zinc-700 text-zinc-200 hover:bg-zinc-600'
            }`}
          >
            {isRecordingShortcut ? 'Recording...' : 'Change Hotkey'}
          </button>
        </section>

        {/* Mic + Language */}
        <section className="bg-zinc-800/50 border border-zinc-700/40 rounded-2xl p-4 space-y-4">
          <div className="flex items-center justify-between">
            <div>
              <h3 className="text-sm font-medium text-zinc-100">Microphone</h3>
              <p className="text-xs text-zinc-500 mt-0.5">Input device</p>
            </div>
            <select
              value={microphoneId}
              onChange={e => { setMicrophoneId(e.target.value); save('microphoneId', e.target.value); }}
              className="bg-zinc-700 border border-zinc-600 text-zinc-100 text-xs rounded-lg p-2 max-w-[140px] truncate focus:outline-none"
            >
              <option value="default">System Default</option>
              {audioDevices.map(d => (
                <option key={d.deviceId} value={d.deviceId}>
                  {d.label || `Mic (${d.deviceId.slice(0,4)}...)`}
                </option>
              ))}
            </select>
          </div>

          <div className="h-px bg-zinc-700/50" />

          <div className="flex items-center justify-between">
            <div>
              <h3 className="text-sm font-medium text-zinc-100">Language</h3>
              <p className="text-xs text-zinc-500 mt-0.5">Transcription language</p>
            </div>
            <select
              value={language}
              onChange={e => { setLanguage(e.target.value); save('language', e.target.value); }}
              className="bg-zinc-700 border border-zinc-600 text-zinc-100 text-xs rounded-lg p-2 focus:outline-none"
            >
              {LANGUAGES.map(l => <option key={l} value={l}>{l}</option>)}
            </select>
          </div>
        </section>

        {/* Gemini API Key (web mode only) */}
        {!isPywebview && (
          <section className="bg-zinc-800/50 border border-zinc-700/40 rounded-2xl p-4">
            <h3 className="text-sm font-medium text-zinc-100 mb-1">Gemini API Key</h3>
            <p className="text-xs text-zinc-500 mb-3">Required for web-mode transcription</p>
            <div className="flex gap-2">
              <input
                type="password" value={apiKey} onChange={e => setApiKey(e.target.value)}
                placeholder="AIzaSy..."
                className="flex-1 bg-zinc-700 border border-zinc-600 text-zinc-100 text-sm rounded-lg p-2.5 focus:outline-none focus:border-blue-500"
              />
              <button
                onClick={() => { localStorage.setItem('dictando_apiKey', apiKey); save('apiKey', apiKey); }}
                className="px-4 bg-blue-600 hover:bg-blue-500 text-white text-sm font-medium rounded-lg transition-colors"
              >Save</button>
            </div>
          </section>
        )}

        {/* Toggles */}
        <section className="bg-zinc-800/50 border border-zinc-700/40 rounded-2xl p-4 space-y-4">
          {[
            { key: 'autoCopy', label: 'Auto-copy', desc: 'Copy to clipboard after transcription', val: autoCopy, set: setAutoCopy },
            { key: 'autoClear', label: 'Auto-clear', desc: 'Clear result after 3 seconds', val: autoClear, set: setAutoClear },
          ].map(({ key, label, desc, val, set }, i) => (
            <React.Fragment key={key}>
              {i > 0 && <div className="h-px bg-zinc-700/50" />}
              <div className="flex items-center justify-between">
                <div>
                  <h3 className="text-sm font-medium text-zinc-100">{label}</h3>
                  <p className="text-xs text-zinc-500 mt-0.5">{desc}</p>
                </div>
                <Toggle checked={val} onChange={v => { set(v); save(key, v); }} />
              </div>
            </React.Fragment>
          ))}
        </section>

        {/* Sign out */}
        <button
          onClick={() => signOut(auth)}
          className="w-full py-3 border border-zinc-800 text-zinc-500 hover:text-red-400 hover:border-red-400/30 rounded-xl text-sm font-medium transition-all flex items-center justify-center gap-2"
        >
          <LogOut size={15} /> Sign Out
        </button>
      </div>
    </motion.div>
  );
}
