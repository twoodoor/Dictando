import React, { useEffect, useState, useRef } from 'react';
import { motion, AnimatePresence } from 'motion/react';
import { Check } from 'lucide-react';
import { events, type RecordingState, isNative } from '../lib/bridge';

/** Number of equalizer bars in the waveform visualizer. */
const BAR_COUNT = 16;

/**
 * Flat, graphic listening pill HUD. The wide audio-reactive equalizer is the
 * hero element — a beautiful artifact to watch while speaking.
 */
export function Overlay() {
  const [state, setState] = useState<RecordingState>('recording');
  const [audioLevel, setAudioLevel] = useState(0);
  const [elapsedSec, setElapsedSec] = useState(0);
  const [showPasted, setShowPasted] = useState(false);
  const smoothedLevelRef = useRef(0);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    const unlistenState = events.onRecordingState((newState) => {
      setState(newState);
      if (newState === 'recording') {
        setElapsedSec(0);
        setShowPasted(false);
      }
    });

    const unlistenAudio = events.onAudioLevel((level) => {
      smoothedLevelRef.current = Math.max(
        level,
        smoothedLevelRef.current * 0.7 + level * 0.3
      );
      setAudioLevel(smoothedLevelRef.current);
    });

    const unlistenText = events.onTranscription((result) => {
      if (result.text) {
        setShowPasted(true);
      }
    });

    return () => {
      unlistenState();
      unlistenAudio();
      unlistenText();
    };
  }, []);

  // Live timer
  useEffect(() => {
    if (state === 'recording') {
      timerRef.current = setInterval(() => {
        setElapsedSec((prev) => prev + 1);
      }, 1000);
    } else {
      if (timerRef.current) clearInterval(timerRef.current);
    }
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, [state]);

  // Smooth decay + browser-preview simulation
  useEffect(() => {
    let animId: number;
    let tick = 0;
    const loop = () => {
      tick += 0.08;
      if (!isNative) {
        const sim =
          (Math.sin(tick) * 0.5 + 0.5) * 0.55 +
          (Math.sin(tick * 2.3) * 0.5 + 0.5) * 0.3 +
          (Math.sin(tick * 3.7) * 0.5 + 0.5) * 0.15;
        setAudioLevel(sim);
      } else {
        smoothedLevelRef.current *= 0.88;
        if (smoothedLevelRef.current < 0.02) smoothedLevelRef.current = 0;
      }
      animId = requestAnimationFrame(loop);
    };
    animId = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(animId);
  }, []);

  const formatTime = (seconds: number) => {
    const m = Math.floor(seconds / 60);
    const s = seconds % 60;
    return `${m}:${s.toString().padStart(2, '0')}`;
  };

  const isRecording = state === 'recording';

  return (
    <div className="w-screen h-screen flex items-center justify-center bg-transparent select-none pointer-events-none p-2 overflow-hidden">
      <motion.div
        layout
        initial={{ opacity: 0, scale: 0.9, y: 8 }}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        exit={{ opacity: 0, scale: 0.92, y: 6 }}
        transition={{
          type: 'spring',
          stiffness: 480,
          damping: 34,
          mass: 0.7,
        }}
        className="relative flex items-center gap-3 px-4 py-2.5 rounded-full bg-zinc-950/95 backdrop-blur-xl border border-white/10 shadow-2xl shadow-black/80"
      >
        <AnimatePresence mode="wait">
          {showPasted ? (
            <motion.div
              key="pasted"
              initial={{ opacity: 0, scale: 0.85 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.85 }}
              className="flex items-center gap-2 px-1 py-0.5"
            >
              <div className="flex items-center justify-center w-4 h-4 rounded-full bg-emerald-500/20 text-emerald-400">
                <Check className="w-3 h-3 stroke-[2.5]" />
              </div>
              <span className="text-[12px] font-medium text-emerald-300 tracking-tight">
                Pasted
              </span>
            </motion.div>
          ) : isRecording ? (
            <motion.div
              key="recording"
              initial={{ opacity: 0, scale: 0.95 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.95 }}
              transition={{ type: 'spring', stiffness: 450, damping: 30 }}
              className="flex items-center gap-3"
            >
              {/* Red recording dot */}
              <div className="relative flex items-center justify-center w-3.5 h-3.5 shrink-0">
                <motion.span
                  animate={{
                    scale: [1, 1.7, 1],
                    opacity: [0.6, 0, 0.6],
                  }}
                  transition={{
                    repeat: Infinity,
                    duration: 1.5,
                    ease: 'easeOut',
                  }}
                  className="absolute w-full h-full rounded-full border border-red-500/60"
                />
                <span className="relative w-2 h-2 rounded-full bg-red-500" />
              </div>

              {/* ── Wide Hero Equalizer ── */}
              <div className="flex items-center gap-[3px] h-6">
                {Array.from({ length: BAR_COUNT }, (_, i) => {
                  // Each bar gets a unique phase so the waveform looks organic
                  const phase = i * 0.45;
                  const minH = 3;
                  const maxH = 22;
                  const boost = audioLevel * (maxH - minH);

                  // Composite wave: two frequencies for natural voice-like motion
                  const w1 = Math.sin(Date.now() * 0.007 + phase) * 0.5 + 0.5;
                  const w2 = Math.sin(Date.now() * 0.013 + phase * 1.7) * 0.5 + 0.5;
                  const wave = w1 * 0.6 + w2 * 0.4;

                  // Bell curve bias: center bars reach higher
                  const center = (BAR_COUNT - 1) / 2;
                  const dist = Math.abs(i - center) / center; // 0 at center, 1 at edges
                  const bell = 1 - dist * 0.45;

                  const h = Math.min(
                    maxH,
                    Math.max(minH, minH + boost * wave * bell)
                  );

                  return (
                    <motion.span
                      key={i}
                      animate={{ height: `${h}px` }}
                      transition={{ type: 'spring', stiffness: 500, damping: 20 }}
                      className="w-[3px] rounded-full bg-zinc-100"
                    />
                  );
                })}
              </div>

              {/* Timer */}
              <span className="text-[11px] font-mono text-zinc-400 tabular-nums shrink-0">
                {formatTime(elapsedSec)}
              </span>
            </motion.div>
          ) : (
            <motion.div
              key="transcribing"
              initial={{ opacity: 0, scale: 0.95 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.95 }}
              transition={{ type: 'spring', stiffness: 450, damping: 30 }}
              className="flex items-center gap-2.5 px-0.5"
            >
              <span className="w-3.5 h-3.5 rounded-full border-2 border-white/15 border-t-white animate-spin shrink-0" />
              <span className="text-[12px] font-medium text-zinc-300 tracking-tight">
                Transcribing…
              </span>
            </motion.div>
          )}
        </AnimatePresence>
      </motion.div>
    </div>
  );
}

