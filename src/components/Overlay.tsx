import React, { useEffect, useState, useRef } from 'react';
import { motion, AnimatePresence } from 'motion/react';
import { Check } from 'lucide-react';
import { events, type RecordingState, isNative } from '../lib/bridge';

/** Number of equalizer bars in the hero waveform. */
const BAR_COUNT = 20;

/**
 * Flat, graphic listening pill HUD with a living, juicy 60 FPS audio visualizer.
 * The equalizer is the centerpiece — dancing organically even at rest and
 * reacting dynamically to your voice in real time.
 */
export function Overlay() {
  const [state, setState] = useState<RecordingState>('recording');
  const [elapsedSec, setElapsedSec] = useState(0);
  const [showPasted, setShowPasted] = useState(false);
  const [barHeights, setBarHeights] = useState<number[]>(() => Array(BAR_COUNT).fill(6));

  const rawLevelRef = useRef(0);
  const smoothedLevelRef = useRef(0);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Subscribe to backend recording state, live audio levels, and paste events
  useEffect(() => {
    const unlistenState = events.onRecordingState((newState) => {
      setState(newState);
      if (newState === 'recording') {
        setElapsedSec(0);
        setShowPasted(false);
      }
    });

    const unlistenAudio = events.onAudioLevel((level) => {
      rawLevelRef.current = level;
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

  // Live timer during recording
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

  // Living 60 FPS waveform animation engine
  useEffect(() => {
    let animId: number;
    let tick = 0;

    const renderLoop = () => {
      tick += 0.07;

      let targetLevel = rawLevelRef.current;
      if (!isNative) {
        // Browser fallback: simulated organic voice fluctuations
        targetLevel =
          (Math.sin(tick * 0.9) * 0.5 + 0.5) * 0.6 +
          (Math.sin(tick * 2.1) * 0.5 + 0.5) * 0.35 +
          (Math.sin(tick * 3.7) * 0.5 + 0.5) * 0.15;
      }

      // Fast attack (quick response to speech), smooth exponential release
      smoothedLevelRef.current =
        smoothedLevelRef.current * 0.72 + targetLevel * 0.28;

      // Natural decay of raw level
      rawLevelRef.current *= 0.92;

      // Compute heights for all 20 bars with center bell-curve bias
      const center = (BAR_COUNT - 1) / 2;
      const heights: number[] = [];

      for (let i = 0; i < BAR_COUNT; i++) {
        const dist = Math.abs(i - center) / center; // 0 at center, 1 at edges
        const bell = Math.cos(dist * (Math.PI / 2.3)); // Smooth bell curve (1.0 -> 0.25)
        const phase = i * 0.38;

        // Base idle breathing wave (4px to 10px) so it's always alive and organic
        const w1 = Math.sin(tick + phase) * 0.5 + 0.5;
        const w2 = Math.sin(tick * 1.6 + phase * 1.8) * 0.5 + 0.5;
        const idle = 4.0 + (w1 * 0.6 + w2 * 0.4) * 6.0 * bell;

        // Dynamic voice surge (up to +18px)
        const energy = Math.pow(smoothedLevelRef.current, 0.75);
        const voiceBoost = energy * 18.0 * bell;
        const voiceFlutter = Math.sin(tick * 2.8 + phase * 2.2) * 0.5 + 0.5;

        const total = Math.min(26, Math.max(3.5, idle + voiceBoost * (0.6 + voiceFlutter * 0.4)));
        heights.push(Math.round(total * 10) / 10);
      }

      setBarHeights(heights);
      animId = requestAnimationFrame(renderLoop);
    };

    animId = requestAnimationFrame(renderLoop);
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
        className="relative flex items-center gap-3.5 px-4 py-2.5 rounded-full bg-zinc-950/95 backdrop-blur-xl border border-white/10 shadow-2xl shadow-black/80 ring-1 ring-white/5"
      >
        <AnimatePresence mode="wait">
          {showPasted ? (
            /* Success / Pasted state */
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
            /* Hero Equalizer Listening State */
            <motion.div
              key="recording"
              initial={{ opacity: 0, scale: 0.95 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.95 }}
              transition={{ type: 'spring', stiffness: 450, damping: 30 }}
              className="flex items-center gap-3.5"
            >
              {/* Pulsing Red Recording Indicator */}
              <div className="relative flex items-center justify-center w-3.5 h-3.5 shrink-0">
                <motion.span
                  animate={{
                    scale: [1, 1.8, 1],
                    opacity: [0.65, 0, 0.65],
                  }}
                  transition={{
                    repeat: Infinity,
                    duration: 1.4,
                    ease: 'easeOut',
                  }}
                  className="absolute w-full h-full rounded-full border border-red-500/60"
                />
                <span className="relative w-2 h-2 rounded-full bg-red-500 shadow-[0_0_6px_rgba(239,68,68,0.7)]" />
              </div>

              {/* ── Wide Hero Equalizer Waveform (20 Bars) ── */}
              <div className="flex items-center gap-[3px] h-7">
                {barHeights.map((h, i) => (
                  <span
                    key={i}
                    style={{ height: `${h}px` }}
                    className="w-[2.5px] rounded-full bg-zinc-100 transition-[height] duration-75 ease-out shrink-0"
                  />
                ))}
              </div>

              {/* Tabular Timer */}
              <span className="text-[11.5px] font-mono text-zinc-400 tabular-nums shrink-0 pl-0.5">
                {formatTime(elapsedSec)}
              </span>
            </motion.div>
          ) : (
            /* Transcribing State */
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

