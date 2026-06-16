import React, { useState, useEffect } from 'react';
import { auth, db } from '../firebase';
import { signOut, User } from 'firebase/auth';
import { doc, setDoc, getDoc } from 'firebase/firestore';
import { LogOut, Keyboard } from 'lucide-react';
import { toast } from 'sonner';
import { motion } from 'motion/react';
import { isNative, backend } from '../lib/bridge';

const LANGUAGES = [
  'Auto-detect', 'English', 'Spanish', 'French', 'German', 'Italian',
  'Portuguese', 'Romanian', 'Dutch', 'Russian', 'Polish', 'Ukrainian',
  'Czech', 'Swedish', 'Danish', 'Finnish', 'Greek',
];

const MODIFIER_CODES = new Set([
  'ControlLeft', 'ControlRight', 'AltLeft', 'AltRight',
  'ShiftLeft', 'ShiftRight', 'MetaLeft', 'MetaRight', 'OSLeft', 'OSRight',
]);

const formatKey = (code: string) =>
  code.replace('Key', '').replace('Digit', '').replace('Left', '').replace('Right', '');

function Toggle({ checked, onChange }: { checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <button
      onClick={() => onChange(!checked)}
      className={`relative w-10 h-5 rounded-full transition-colors ${checked ? 'bg-accent' : 'bg-surface-2 border border-line'}`}
    >
      <span className={`absolute top-0.5 left-0.5 w-4 h-4 rounded-full bg-white shadow transition-transform ${checked ? 'translate-x-5' : ''}`} />
    </button>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="mb-6">
      <h2 className="text-[11px] uppercase tracking-wide text-faint mb-2 px-1">{title}</h2>
      <div className="bg-surface border border-line rounded-2xl divide-y divide-[color:var(--line)]">{children}</div>
    </section>
  );
}

function Row({ title, desc, children }: { title: string; desc?: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4 px-4 py-3.5">
      <div className="min-w-0">
        <div className="text-sm font-medium text-fg">{title}</div>
        {desc && <div className="text-xs text-muted mt-0.5">{desc}</div>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

const selectCls = 'bg-surface-2 border border-line text-fg text-xs rounded-lg px-2 py-1.5 max-w-[180px] truncate focus:outline-none focus:border-accent';

export function SettingsView({ user }: { user: User | null }) {
  const [shortcut, setShortcut] = useState<string[]>(['ControlLeft', 'Space']);
  const [pushToTalk, setPushToTalk] = useState(true);
  const [language, setLanguage] = useState('Auto-detect');
  const [microphoneId, setMicrophoneId] = useState('default');
  const [pasteMethod, setPasteMethod] = useState('direct');
  const [appendSpace, setAppendSpace] = useState(false);
  const [unloadMinutes, setUnloadMinutes] = useState(5);
  const [historyLimit, setHistoryLimit] = useState(50);
  const [apiKey, setApiKey] = useState('');
  const [launchOnStartup, setLaunchOnStartup] = useState(false);
  const [aiEnhance, setAiEnhance] = useState(false);
  const [geminiKey, setGeminiKey] = useState('');
  const [mics, setMics] = useState<{ id: string; label: string }[]>([]);
  const [capturing, setCapturing] = useState(false);
  const [pressed, setPressed] = useState<string[]>([]);

  // Load settings.
  useEffect(() => {
    if (isNative) {
      backend.getSettings().then((s) => {
        if (s.shortcut?.length) setShortcut(s.shortcut);
        setPushToTalk(s.pushToTalk);
        setLanguage(s.language || 'Auto-detect');
        setMicrophoneId(s.microphoneId || 'default');
        setPasteMethod(s.pasteMethod || 'direct');
        setAppendSpace(!!s.appendTrailingSpace);
        setUnloadMinutes(s.unloadModelMinutes ?? 5);
        setHistoryLimit(s.historyLimit ?? 50);
        setLaunchOnStartup(!!s.launchOnStartup);
        setAiEnhance(!!s.aiEnhanceEnabled);
        setGeminiKey(s.geminiApiKey || '');
      }).catch(() => {});
      return;
    }
    if (!user) return;
    getDoc(doc(db, 'users', user.uid)).then((snap) => {
      if (!snap.exists()) return;
      const d = snap.data();
      if (d.shortcut) setShortcut(Array.isArray(d.shortcut) ? d.shortcut : [d.shortcut]);
      if (d.language) setLanguage(d.language);
      if (d.microphoneId) setMicrophoneId(d.microphoneId);
      if (d.apiKey) setApiKey(d.apiKey);
    }).catch(() => {});
  }, [user]);

  // Microphones.
  useEffect(() => {
    if (isNative) { backend.listMicrophones().then(setMics).catch(() => {}); return; }
    navigator.mediaDevices?.getUserMedia({ audio: true })
      .then((s) => { s.getTracks().forEach((t) => t.stop()); return navigator.mediaDevices.enumerateDevices(); })
      .then((devs) => setMics(devs.filter((d) => d.kind === 'audioinput').map((d) => ({ id: d.deviceId, label: d.label || 'Microphone' }))))
      .catch(() => {});
  }, []);

  const save = async (key: string, value: any) => {
    if (isNative) await backend.updateSettings({ [key]: value } as any).catch(() => {});
    if (user) await setDoc(doc(db, 'users', user.uid), { [key]: value }, { merge: true }).catch(() => {});
  };

  // --- Hotkey capture: finalize when a non-modifier key is pressed ---
  useEffect(() => {
    if (!capturing) return;
    const mods: string[] = [];
    const down = (e: KeyboardEvent) => {
      e.preventDefault();
      if (MODIFIER_CODES.has(e.code)) {
        if (!mods.includes(e.code)) mods.push(e.code);
        setPressed([...mods]);
      } else {
        const combo = [...mods, e.code];
        setShortcut(combo);
        setCapturing(false);
        setPressed([]);
        save('shortcut', combo);
        toast.success(`Hotkey set to ${combo.map(formatKey).join(' + ')}`);
      }
    };
    const up = () => {};
    const cancel = (e: KeyboardEvent) => { if (e.key === 'Escape') { setCapturing(false); setPressed([]); } };
    window.addEventListener('keydown', down, true);
    window.addEventListener('keyup', up, true);
    window.addEventListener('keydown', cancel, true);
    return () => {
      window.removeEventListener('keydown', down, true);
      window.removeEventListener('keyup', up, true);
      window.removeEventListener('keydown', cancel, true);
    };
  }, [capturing]);

  const shownKeys = capturing ? pressed : shortcut;

  return (
    <motion.div
      initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -8 }}
      className="absolute inset-0 overflow-y-auto"
    >
      <div className="max-w-3xl mx-auto px-8 py-8">
        <h1 className="font-display text-3xl text-fg mb-6">Settings</h1>

        <Section title="Dictation">
          <Row title="Hotkey" desc={pushToTalk ? 'Hold to record, release to transcribe' : 'Press to start, press again to stop'}>
            <div className="flex items-center gap-2">
              <div className="flex gap-1 min-h-[28px] items-center">
                {shownKeys.length > 0 ? shownKeys.map((k) => (
                  <kbd key={k} className="px-2 py-1 bg-surface-2 border border-line rounded-lg text-xs font-mono text-fg">{formatKey(k)}</kbd>
                )) : <span className="text-xs text-faint animate-pulse">Press keys…</span>}
              </div>
              <button
                onClick={() => { setCapturing(true); setPressed([]); }}
                className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-colors ${
                  capturing ? 'bg-accent text-accent-fg' : 'bg-surface-2 text-fg hover:bg-surface-2 border border-line'
                }`}
              >
                <Keyboard size={13} /> {capturing ? 'Listening…' : 'Change'}
              </button>
            </div>
          </Row>
          <Row title="Push to talk" desc="Hold the hotkey while speaking">
            <Toggle checked={pushToTalk} onChange={(v) => { setPushToTalk(v); save('pushToTalk', v); }} />
          </Row>
          <Row title="Language" desc="Transcription language">
            <select value={language} onChange={(e) => { setLanguage(e.target.value); save('language', e.target.value); }} className={selectCls}>
              {LANGUAGES.map((l) => <option key={l} value={l}>{l}</option>)}
            </select>
          </Row>
        </Section>

        <Section title="Audio">
          <Row title="Microphone" desc="Input device">
            <select value={microphoneId} onChange={(e) => { setMicrophoneId(e.target.value); save('microphoneId', e.target.value); }} className={selectCls}>
              <option value="default">System Default</option>
              {mics.filter((d) => d.id !== 'default').map((d) => <option key={d.id} value={d.id}>{d.label}</option>)}
            </select>
          </Row>
        </Section>

        {isNative && (
          <Section title="Output">
            <Row title="Paste method" desc="How text is inserted into other apps">
              <select value={pasteMethod} onChange={(e) => { setPasteMethod(e.target.value); save('pasteMethod', e.target.value); }} className={selectCls}>
                <option value="direct">Type directly</option>
                <option value="clipboard">Paste (clipboard)</option>
              </select>
            </Row>
            <Row title="Append trailing space" desc="Add a space after pasted text">
              <Toggle checked={appendSpace} onChange={(v) => { setAppendSpace(v); save('appendTrailingSpace', v); }} />
            </Row>
          </Section>
        )}

        {isNative && (
          <Section title="Models & history">
            <Row title="Unload model when idle" desc="Free memory after inactivity">
              <select value={unloadMinutes} onChange={(e) => { const v = Number(e.target.value); setUnloadMinutes(v); save('unloadModelMinutes', v); }} className={selectCls}>
                <option value={0}>Never</option>
                <option value={1}>After 1 minute</option>
                <option value={5}>After 5 minutes</option>
                <option value={15}>After 15 minutes</option>
              </select>
            </Row>
            <Row title="History limit" desc="Keep this many recent transcriptions">
              <select value={historyLimit} onChange={(e) => { const v = Number(e.target.value); setHistoryLimit(v); save('historyLimit', v); }} className={selectCls}>
                {[20, 50, 100, 500].map((n) => <option key={n} value={n}>{n} entries</option>)}
              </select>
            </Row>
          </Section>
        )}

        {isNative && (
          <Section title="App">
            <Row title="Launch on startup" desc="Start Dictando when you log in">
              <Toggle checked={launchOnStartup} onChange={(v) => { setLaunchOnStartup(v); save('launchOnStartup', v); }} />
            </Row>
          </Section>
        )}

        {isNative && (
          <Section title="AI enhancement">
            <Row title="Clean up with AI" desc="Fix punctuation & remove fillers via Gemini (off = fully offline)">
              <Toggle checked={aiEnhance} onChange={(v) => { setAiEnhance(v); save('aiEnhanceEnabled', v); }} />
            </Row>
            <div className="px-4 py-3.5">
              <div className="text-sm font-medium text-fg">Gemini API key</div>
              <div className="text-xs text-muted mt-0.5 mb-2">Stored locally. Required for AI cleanup; raw transcription never leaves your device.</div>
              <div className="flex gap-2">
                <input
                  type="password" value={geminiKey} onChange={(e) => setGeminiKey(e.target.value)} placeholder="AIzaSy…"
                  className="flex-1 bg-surface-2 border border-line text-fg text-sm rounded-lg px-3 py-2 focus:outline-none focus:border-accent"
                />
                <button
                  onClick={() => { save('geminiApiKey', geminiKey); toast.success('Saved'); }}
                  className="px-4 bg-accent text-accent-fg text-sm font-medium rounded-lg hover:bg-accent-strong transition-colors"
                >Save</button>
              </div>
            </div>
          </Section>
        )}

        {!isNative && (
          <Section title="Gemini API key">
            <div className="px-4 py-3.5">
              <p className="text-xs text-muted mb-2">Required for web-mode transcription.</p>
              <div className="flex gap-2">
                <input
                  type="password" value={apiKey} onChange={(e) => setApiKey(e.target.value)} placeholder="AIzaSy…"
                  className="flex-1 bg-surface-2 border border-line text-fg text-sm rounded-lg px-3 py-2 focus:outline-none focus:border-accent"
                />
                <button
                  onClick={() => { localStorage.setItem('dictando_apiKey', apiKey); save('apiKey', apiKey); toast.success('Saved'); }}
                  className="px-4 bg-accent text-accent-fg text-sm font-medium rounded-lg hover:bg-accent-strong transition-colors"
                >Save</button>
              </div>
            </div>
          </Section>
        )}

        {user && (
          <button
            onClick={() => signOut(auth)}
            className="w-full py-3 border border-line text-muted hover:text-danger hover:border-danger/40 rounded-xl text-sm font-medium transition-all flex items-center justify-center gap-2"
          >
            <LogOut size={15} /> Sign out
          </button>
        )}
      </div>
    </motion.div>
  );
}
