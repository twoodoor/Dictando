import React, { useState, useEffect, useRef } from 'react';
import { db } from '../firebase';
import { doc, onSnapshot, addDoc, collection, serverTimestamp } from 'firebase/firestore';
import { User } from 'firebase/auth';
import { Mic, Square, Loader2, Copy } from 'lucide-react';
import { toast } from 'sonner';
import { motion, AnimatePresence } from 'motion/react';
import { GoogleGenAI } from '@google/genai';
import { isNative, backend, events } from '../lib/bridge';

function AudioVisualizer({ isRecording }: { isRecording: boolean }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const rafRef = useRef<number>(0);
  const streamRef = useRef<MediaStream | null>(null);

  useEffect(() => {
    if (!isRecording) {
      cancelAnimationFrame(rafRef.current);
      streamRef.current?.getTracks().forEach((t) => t.stop());
      const c = canvasRef.current;
      if (c) { const ctx = c.getContext('2d')!; ctx.clearRect(0, 0, c.width, c.height); }
      return;
    }
    let audioCtx: AudioContext;
    (async () => {
      try {
        const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
        streamRef.current = stream;
        audioCtx = new AudioContext();
        const source = audioCtx.createMediaStreamSource(stream);
        const analyser = audioCtx.createAnalyser();
        analyser.fftSize = 64;
        source.connect(analyser);
        const data = new Uint8Array(analyser.frequencyBinCount);
        const canvas = canvasRef.current!;
        const ctx = canvas.getContext('2d')!;
        const draw = () => {
          rafRef.current = requestAnimationFrame(draw);
          analyser.getByteFrequencyData(data);
          ctx.clearRect(0, 0, canvas.width, canvas.height);
          const barW = canvas.width / data.length;
          data.forEach((v, i) => {
            const h = (v / 255) * canvas.height * 0.85;
            const x = i * barW;
            const alpha = 0.5 + (v / 255) * 0.5;
            ctx.fillStyle = `rgba(239,68,68,${alpha})`;
            ctx.beginPath();
            ctx.roundRect(x + 1, canvas.height - h, barW - 2, h, 2);
            ctx.fill();
          });
        };
        draw();
      } catch { /* mic denied */ }
    })();
    return () => {
      cancelAnimationFrame(rafRef.current);
      streamRef.current?.getTracks().forEach((t) => t.stop());
    };
  }, [isRecording]);

  return (
    <canvas
      ref={canvasRef}
      width={220}
      height={44}
      className={`transition-opacity duration-300 ${isRecording ? 'opacity-100' : 'opacity-0'}`}
    />
  );
}

export function RecordView({ user }: { user: User | null }) {
  const [isRecording, setIsRecording] = useState(false);
  const [isProcessing, setIsProcessing] = useState(false);
  const [transcription, setTranscription] = useState<string | null>(null);
  const [lastEngine, setLastEngine] = useState<string | null>(null);
  const [modelStatus, setModelStatus] = useState<'loading' | 'ready' | null>(null);
  const [settings, setSettings] = useState({
    shortcut: ['ControlLeft', 'Space'], language: 'Auto-detect',
    autoCopy: true, autoClear: false, microphoneId: 'default', apiKey: '',
  });
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const audioChunksRef = useRef<Blob[]>([]);
  const startTimeRef = useRef<number>(0);
  const pressedKeys = useRef<Set<string>>(new Set());
  const apiKeyRef = useRef<string>(localStorage.getItem('dictando_apiKey') || '');

  // Native (Tauri): drive UI from backend events + load native settings.
  useEffect(() => {
    if (!isNative) return;
    backend.getStatus().then((s) => setModelStatus(s.modelLoaded ? 'ready' : 'loading')).catch(() => {});
    backend.getSettings().then((s) => {
      setSettings((prev) => ({
        ...prev,
        shortcut: s.shortcut?.length ? s.shortcut : prev.shortcut,
        language: s.language || prev.language,
        microphoneId: s.microphoneId || prev.microphoneId,
      }));
    }).catch(() => {});
    const offState = events.onRecordingState((state) => {
      setIsRecording(state === 'recording');
      setIsProcessing(state === 'transcribing');
    });
    const offText = events.onTranscription((r) => {
      setTranscription(r.text);
      setLastEngine(r.engine || 'local');
    });
    const offModel = events.onModelStatus((s) => setModelStatus(s.modelLoaded ? 'ready' : 'loading'));
    return () => { offState(); offText(); offModel(); };
  }, []);

  // Web: load settings from Firestore (skipped without a logged-in user).
  useEffect(() => {
    if (isNative || !user) return;
    const unsub = onSnapshot(doc(db, 'users', user.uid), (snap) => {
      if (snap.exists()) {
        const d = snap.data();
        const key = d.apiKey || localStorage.getItem('dictando_apiKey') || '';
        apiKeyRef.current = key;
        setSettings({
          shortcut: Array.isArray(d.shortcut) ? d.shortcut : ['ControlLeft', 'Space'],
          language: d.language || 'Auto-detect',
          autoCopy: d.autoCopy !== undefined ? d.autoCopy : true,
          autoClear: d.autoClear !== undefined ? d.autoClear : false,
          microphoneId: d.microphoneId || 'default',
          apiKey: key,
        });
      }
    }, () => {});
    return unsub;
  }, [user]);

  // Web keyboard shortcut (native uses the OS-level global shortcut).
  useEffect(() => {
    if (isNative) return;
    const down = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
      pressedKeys.current.add(e.code);
      const all = settings.shortcut.every((k) => pressedKeys.current.has(k));
      if (all && !isRecording && !isProcessing) { e.preventDefault(); startRecording(); }
    };
    const up = (e: KeyboardEvent) => {
      pressedKeys.current.delete(e.code);
      if (settings.shortcut.includes(e.code) && isRecording) { e.preventDefault(); stopRecording(); }
    };
    const blur = () => { pressedKeys.current.clear(); if (isRecording) stopRecording(); };
    window.addEventListener('keydown', down);
    window.addEventListener('keyup', up);
    window.addEventListener('blur', blur);
    return () => {
      window.removeEventListener('keydown', down);
      window.removeEventListener('keyup', up);
      window.removeEventListener('blur', blur);
    };
  }, [isRecording, isProcessing, settings.shortcut]);

  const startRecording = async () => {
    if (isNative) { backend.startRecording().catch(() => toast.error('Failed to start')); return; }
    try {
      const constraint = settings.microphoneId && settings.microphoneId !== 'default'
        ? { deviceId: { exact: settings.microphoneId } } : true;
      const stream = await navigator.mediaDevices.getUserMedia({ audio: constraint });
      const opts = MediaRecorder.isTypeSupported('audio/webm') ? { mimeType: 'audio/webm' } : undefined;
      const mr = new MediaRecorder(stream, opts);
      mediaRecorderRef.current = mr;
      audioChunksRef.current = [];
      startTimeRef.current = Date.now();
      mr.ondataavailable = (e) => { if (e.data.size > 0) audioChunksRef.current.push(e.data); };
      mr.onstop = async () => {
        const durationMs = Date.now() - startTimeRef.current;
        stream.getTracks().forEach((t) => t.stop());
        if (!audioChunksRef.current.length) { toast.error('No audio detected'); setIsProcessing(false); return; }
        const blob = new Blob(audioChunksRef.current, { type: mr.mimeType || 'audio/webm' });
        await processAudio(blob, durationMs);
      };
      mr.start();
      setIsRecording(true);
      setTranscription(null);
    } catch { toast.error('Microphone access denied'); }
  };

  const stopRecording = () => {
    if (isNative) { backend.stopRecording().catch(() => {}); return; }
    if (mediaRecorderRef.current && isRecording) {
      mediaRecorderRef.current.stop();
      setIsRecording(false);
      setIsProcessing(true);
    }
  };

  const processAudio = async (blob: Blob, durationMs: number) => {
    try {
      const apiKey = apiKeyRef.current || (import.meta as any).env.VITE_GEMINI_API_KEY;
      if (!apiKey) { toast.error('No API key — set one in Settings'); setIsProcessing(false); return; }
      if (blob.size < 100) { toast.error('Recording too short'); setIsProcessing(false); return; }
      const reader = new FileReader();
      reader.readAsDataURL(blob);
      reader.onloadend = async () => {
        const b64 = (reader.result as string).split(',')[1];
        try {
          const genAI = new GoogleGenAI({ apiKey });
          const lang = settings.language;
          const res = await genAI.models.generateContent({
            model: 'gemini-2.0-flash',
            contents: { parts: [
              { inlineData: { mimeType: blob.type.split(';')[0] || 'audio/webm', data: b64 } },
              { text: `Transcribe exactly as spoken.${lang !== 'Auto-detect' ? ` Language: ${lang}.` : ''} Output raw text only.` },
            ] },
          });
          const text = res.text?.trim() || '';
          setTranscription(text);
          setLastEngine('gemini');
          if (text) {
            if (user) {
              await addDoc(collection(db, `users/${user.uid}/dictations`), {
                text, durationMs, engine: 'gemini', timestamp: serverTimestamp(),
              }).catch(() => {});
            }
            if (settings.autoCopy) {
              await navigator.clipboard.writeText(text).catch(() => {});
              toast.success('Copied to clipboard!');
            }
            if (settings.autoClear) setTimeout(() => setTranscription(null), 3000);
          }
        } catch (err: any) {
          toast.error(err?.message?.includes('API key') ? 'Invalid API key' : 'Transcription failed');
        } finally { setIsProcessing(false); }
      };
    } catch { setIsProcessing(false); toast.error('Processing error'); }
  };

  const shortcutLabel = settings.shortcut
    .map((k) => k.replace('Key', '').replace('Digit', '').replace('Left', '').replace('Right', ''))
    .join(' + ');

  return (
    <motion.div
      initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -8 }}
      className="absolute inset-0 overflow-y-auto"
    >
      <div className="min-h-full flex flex-col items-center justify-center px-8 py-10 max-w-xl mx-auto text-center gap-7">
        {/* Model status (desktop only) */}
        {isNative && modelStatus && (
          <div className={`flex items-center gap-2 text-xs px-3 py-1.5 rounded-full border ${
            modelStatus === 'ready'
              ? 'bg-surface border-line text-fg'
              : 'bg-surface-2 border-line text-muted'
          }`}>
            <span className={`w-1.5 h-1.5 rounded-full ${modelStatus === 'ready' ? 'bg-emerald-500' : 'bg-amber-500 animate-pulse'}`} />
            {modelStatus === 'ready' ? 'Model ready' : 'Loading model…'}
          </div>
        )}

        <div>
          <h1 className="font-display text-3xl text-fg">Ready when you are</h1>
          <div className="h-6 mt-2 flex items-center justify-center">
            {isRecording ? (
              <span className="text-danger font-medium text-sm flex items-center gap-2">
                <span className="w-2 h-2 rounded-full bg-danger animate-pulse" /> Recording…
              </span>
            ) : isProcessing ? (
              <span className="text-accent font-medium text-sm flex items-center gap-2">
                <Loader2 className="w-4 h-4 animate-spin" /> Transcribing…
              </span>
            ) : (
              <span className="text-muted text-sm flex items-center gap-1.5">
                Hold <kbd className="px-1.5 py-0.5 bg-surface-2 border border-line rounded text-[11px] font-mono text-fg">{shortcutLabel}</kbd> to dictate
              </span>
            )}
          </div>
        </div>

        <AudioVisualizer isRecording={isRecording} />

        <button
          onMouseDown={startRecording} onMouseUp={stopRecording} onMouseLeave={stopRecording}
          onTouchStart={startRecording} onTouchEnd={stopRecording}
          disabled={isProcessing}
          className={`relative w-32 h-32 rounded-full flex items-center justify-center transition-all duration-300 ${
            isRecording
              ? 'bg-red-500/15 text-danger scale-110 shadow-[0_0_50px_rgba(239,68,68,0.25)]'
              : isProcessing
                ? 'bg-surface-2 text-faint cursor-not-allowed'
                : 'bg-accent-soft text-accent hover:scale-105 shadow-xl'
          }`}
        >
          {isRecording ? <Square size={38} className="fill-current" /> : <Mic size={46} />}
          {isRecording && (
            <>
              <div className="absolute inset-0 rounded-full border-2 border-red-500/40 animate-ping" style={{ animationDuration: '1.5s' }} />
              <div className="absolute inset-[-16px] rounded-full border border-red-500/20 animate-ping" style={{ animationDuration: '2s', animationDelay: '0.5s' }} />
            </>
          )}
        </button>

        {/* Latest result */}
        <div className="w-full min-h-[60px]">
          <AnimatePresence>
            {transcription && (
              <motion.div
                initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }}
                className="w-full bg-surface border border-line rounded-2xl p-4 text-left relative group"
              >
                <p className="text-fg text-sm leading-relaxed pr-8">{transcription}</p>
                <button
                  onClick={() => navigator.clipboard.writeText(transcription).then(() => toast.success('Copied!'))}
                  className="absolute top-3 right-3 p-1.5 bg-surface-2 text-muted rounded-md opacity-0 group-hover:opacity-100 transition-opacity hover:text-fg"
                >
                  <Copy size={13} />
                </button>
                {lastEngine && (
                  <div className="mt-2 text-[10px] font-mono text-faint uppercase tracking-wide">{lastEngine}</div>
                )}
              </motion.div>
            )}
          </AnimatePresence>
        </div>
      </div>
    </motion.div>
  );
}
