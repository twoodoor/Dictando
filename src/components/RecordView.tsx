import React, { useState, useEffect, useRef } from 'react';
import { db } from '../firebase';
import { doc, onSnapshot, addDoc, collection, serverTimestamp } from 'firebase/firestore';
import { User } from 'firebase/auth';
import { Mic, Square, Loader2, Copy } from 'lucide-react';
import { toast } from 'sonner';
import { motion, AnimatePresence } from 'motion/react';
import { GoogleGenAI } from '@google/genai';

const ENGINE_LABELS: Record<string, string> = {
  'faster-whisper': 'Local',
  'google': 'Google',
  'gemini': 'Gemini',
};

function EngineBadge({ engine }: { engine: string | null }) {
  if (!engine) return null;
  const label = ENGINE_LABELS[engine] || engine;
  const colour = engine === 'faster-whisper'
    ? 'bg-emerald-500/15 text-emerald-400 border-emerald-500/30'
    : 'bg-blue-500/15 text-blue-400 border-blue-500/30';
  return (
    <span className={`text-[10px] font-mono px-1.5 py-0.5 rounded border ${colour}`}>
      {label}
    </span>
  );
}

function AudioVisualizer({ isRecording }: { isRecording: boolean }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const rafRef = useRef<number>(0);
  const analyserRef = useRef<AnalyserNode | null>(null);
  const streamRef = useRef<MediaStream | null>(null);

  useEffect(() => {
    if (!isRecording) {
      cancelAnimationFrame(rafRef.current);
      streamRef.current?.getTracks().forEach(t => t.stop());
      // Clear canvas
      const c = canvasRef.current;
      if (c) { const ctx = c.getContext('2d')!; ctx.clearRect(0, 0, c.width, c.height); }
      return;
    }

    let ctx_: AudioContext;
    (async () => {
      try {
        const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
        streamRef.current = stream;
        ctx_ = new AudioContext();
        const source = ctx_.createMediaStreamSource(stream);
        const analyser = ctx_.createAnalyser();
        analyser.fftSize = 64;
        source.connect(analyser);
        analyserRef.current = analyser;

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
      streamRef.current?.getTracks().forEach(t => t.stop());
    };
  }, [isRecording]);

  return (
    <canvas
      ref={canvasRef}
      width={200}
      height={40}
      className={`transition-opacity duration-300 ${isRecording ? 'opacity-100' : 'opacity-0'}`}
    />
  );
}

export function RecordView({ user }: { user: User }) {
  const [isRecording, setIsRecording] = useState(false);
  const [isProcessing, setIsProcessing] = useState(false);
  const [transcription, setTranscription] = useState<string | null>(null);
  const [lastEngine, setLastEngine] = useState<string | null>(null);
  const [modelStatus, setModelStatus] = useState<'loading' | 'ready' | null>(null);
  const [settings, setSettings] = useState({
    shortcut: ['Space'], language: 'Auto-detect',
    autoCopy: true, autoClear: false,
    microphoneId: 'default', apiKey: '',
  });
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const audioChunksRef = useRef<Blob[]>([]);
  const startTimeRef = useRef<number>(0);
  const pressedKeys = useRef<Set<string>>(new Set());
  const apiKeyRef = useRef<string>(localStorage.getItem('dictando_apiKey') || '');

  // Poll Python model status
  useEffect(() => {
    const pyApi = (window as any).pywebview?.api;
    if (!pyApi) return;
    setModelStatus('loading');
    const iv = setInterval(async () => {
      try {
        const raw = await pyApi.get_status();
        const s = JSON.parse(raw);
        if (s.modelLoaded) { setModelStatus('ready'); clearInterval(iv); }
      } catch { /* not ready yet */ }
    }, 1500);
    return () => clearInterval(iv);
  }, []);

  // Listen for dictandoStatus events
  useEffect(() => {
    const h = (e: Event) => {
      const s = (e as CustomEvent).detail;
      if (s.modelLoaded) setModelStatus('ready');
    };
    window.addEventListener('dictandoStatus', h);
    return () => window.removeEventListener('dictandoStatus', h);
  }, []);

  // Load settings from Firestore
  useEffect(() => {
    const unsub = onSnapshot(doc(db, 'users', user.uid), (snap) => {
      if (snap.exists()) {
        const d = snap.data();
        const key = d.apiKey || localStorage.getItem('dictando_apiKey') || '';
        apiKeyRef.current = key;
        setSettings({
          shortcut: Array.isArray(d.shortcut) ? d.shortcut : ['Space'],
          language: d.language || 'Auto-detect',
          autoCopy: d.autoCopy !== undefined ? d.autoCopy : true,
          autoClear: d.autoClear !== undefined ? d.autoClear : false,
          microphoneId: d.microphoneId || 'default',
          apiKey: key,
        });
      }
    }, () => {});
    return unsub;
  }, [user.uid]);

  // Keyboard shortcut handler
  useEffect(() => {
    const down = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
      pressedKeys.current.add(e.code);
      const all = settings.shortcut.every(k => pressedKeys.current.has(k));
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
        stream.getTracks().forEach(t => t.stop());
        if (!audioChunksRef.current.length) {
          toast.error('No audio detected'); setIsProcessing(false); return;
        }
        const blob = new Blob(audioChunksRef.current, { type: mr.mimeType || 'audio/webm' });
        await processAudio(blob, durationMs);
      };
      mr.start();
      setIsRecording(true);
      setTranscription(null);
    } catch { toast.error('Microphone access denied'); }
  };

  const stopRecording = () => {
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
              { text: `Transcribe exactly as spoken.${lang !== 'Auto-detect' ? ` Language: ${lang}.` : ''} Output raw text only.` }
            ]}
          });
          const text = res.text?.trim() || '';
          setTranscription(text);
          setLastEngine('gemini');

          if (text) {
            await addDoc(collection(db, `users/${user.uid}/dictations`), {
              text, durationMs, engine: 'gemini', timestamp: serverTimestamp(),
            }).catch(() => {});
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
    .map(k => k.replace('Key','').replace('Digit','').replace('Left','').replace('Right',''))
    .join(' + ');

  const isPywebview = !!(window as any).pywebview;

  return (
    <motion.div
      initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -8 }}
      className="absolute inset-0 flex flex-col items-center justify-center p-6 gap-6"
    >
      {/* Model status (desktop only) */}
      {isPywebview && modelStatus && (
        <div className={`flex items-center gap-2 text-xs px-3 py-1.5 rounded-full border ${
          modelStatus === 'ready'
            ? 'bg-emerald-500/10 border-emerald-500/20 text-emerald-400'
            : 'bg-zinc-800 border-zinc-700 text-zinc-400'
        }`}>
          <span className={`w-1.5 h-1.5 rounded-full ${
            modelStatus === 'ready' ? 'bg-emerald-400' : 'bg-zinc-500 animate-pulse'
          }`} />
          {modelStatus === 'ready' ? 'Whisper ready' : 'Loading model...'}
        </div>
      )}

      {/* Status text */}
      <div className="h-7 flex items-center justify-center">
        {isRecording ? (
          <span className="text-red-400 font-medium text-sm flex items-center gap-2">
            <span className="w-2 h-2 rounded-full bg-red-500 animate-pulse" /> Recording...
          </span>
        ) : isProcessing ? (
          <span className="text-blue-400 font-medium text-sm flex items-center gap-2">
            <Loader2 className="w-4 h-4 animate-spin" /> Transcribing...
          </span>
        ) : (
          <span className="text-zinc-500 text-xs flex items-center gap-1.5">
            Hold <kbd className="px-1.5 py-0.5 bg-zinc-800 border border-zinc-700 rounded text-[10px] font-mono">{shortcutLabel}</kbd> to dictate
          </span>
        )}
      </div>

      {/* Waveform */}
      <AudioVisualizer isRecording={isRecording} />

      {/* Record button */}
      <button
        onMouseDown={startRecording} onMouseUp={stopRecording} onMouseLeave={stopRecording}
        onTouchStart={startRecording} onTouchEnd={stopRecording}
        disabled={isProcessing}
        className={`relative w-28 h-28 rounded-full flex items-center justify-center transition-all duration-300 ${
          isRecording
            ? 'bg-red-500/20 text-red-400 scale-110 shadow-[0_0_40px_rgba(239,68,68,0.25)]'
            : isProcessing
              ? 'bg-zinc-800 text-zinc-600 cursor-not-allowed'
              : 'bg-zinc-800 text-zinc-300 hover:bg-zinc-700 hover:scale-105 shadow-xl'
        }`}
      >
        {isRecording ? <Square size={36} className="fill-current" /> : <Mic size={44} />}
        {isRecording && (
          <>
            <div className="absolute inset-0 rounded-full border-2 border-red-500/40 animate-ping" style={{ animationDuration: '1.5s' }} />
            <div className="absolute inset-[-16px] rounded-full border border-red-500/20 animate-ping" style={{ animationDuration: '2s', animationDelay: '0.5s' }} />
          </>
        )}
      </button>

      {/* Result */}
      <div className="w-full min-h-[80px]">
        <AnimatePresence>
          {transcription && (
            <motion.div
              initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }}
              className="w-full bg-zinc-800/60 border border-zinc-700/50 rounded-xl p-4 relative group"
            >
              <div className="flex items-start justify-between gap-2">
                <p className="text-zinc-200 text-sm leading-relaxed flex-1">{transcription}</p>
                <button
                  onClick={() => navigator.clipboard.writeText(transcription).then(() => toast.success('Copied!'))}
                  className="p-1.5 bg-zinc-700/80 text-zinc-400 rounded-md opacity-0 group-hover:opacity-100 transition-opacity hover:text-white shrink-0"
                >
                  <Copy size={13} />
                </button>
              </div>
              {lastEngine && (
                <div className="mt-2">
                  <EngineBadge engine={lastEngine} />
                </div>
              )}
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </motion.div>
  );
}
