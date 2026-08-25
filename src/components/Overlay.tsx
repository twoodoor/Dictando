import React, { useEffect, useState, useRef } from 'react';
import { motion, AnimatePresence } from 'motion/react';
import { Sparkles, Check } from 'lucide-react';
import { events, type RecordingState, isNative } from '../lib/bridge';

/**
 * Dynamic, animated, juicy listening pill HUD rendered in the always-on-top
 * transparent overlay window. Positioned dynamically on whichever display
 * currently holds the user's cursor.
 */
export function Overlay() {
  const [state, setState] = useState<RecordingState>('recording');
  const [audioLevel, setAudioLevel] = useState(0);
  const [elapsedSec, setElapsedSec] = useState(0);
  const [showPasted, setShowPasted] = useState(false);
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

  // Live timer while in recording state
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

  // Ambient fluid oscillation for equalizer bars (and browser fallback)
  useEffect(() => {
    let animId: number;
    let tick = 0;
    const loop = () => {
      tick += 0.08;
      if (!isNative) {
        const sim = (Math.sin(tick) * 0.5 + 0.5) * 0.6 + (Math.sin(tick * 2.3) * 0.5 + 0.5) * 0.4;
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
  const isTranscribing = state === 'transcribing';

  return (
    <div className="w-screen h-screen flex items-center justify-center bg-transparent select-none pointer-events-none p-2 overflow-hidden">
      {/* Outer ambient glow aura matching active state */}
      <div className="relative flex items-center justify-center">
        <motion.div
          animate={{
            scale: isRecording ? 1 + audioLevel * 0.35 : 1,
            opacity: isRecording ? 0.45 + audioLevel * 0.45 : isTranscribing ? 0.55 : 0.2,
          }}
          transition={{ type: 'spring', stiffness: 300, damping: 25 }}
          className={`absolute inset-0 -m-6 rounded-full blur-2xl transition-colors duration-500 pointer-events-none ${
            showPasted
              ? 'bg-emerald-500/50'
              : isTranscribing
              ? 'bg-gradient-to-r from-violet-600/40 via-fuchsia-500/40 to-cyan-400/40'
              : 'bg-gradient-to-r from-rose-500/40 via-violet-600/35 to-amber-500/35'
          }`}
        />

        {/* Dynamic Glass Pill Container */}
        <motion.div
          layout
          initial={{ opacity: 0, scale: 0.85, y: 14 }}
          animate={{ opacity: 1, scale: 1, y: 0 }}
          exit={{ opacity: 0, scale: 0.88, y: 10 }}
          transition={{
            type: 'spring',
            stiffness: 450,
            damping: 32,
            mass: 0.8,
          }}
          className="relative flex items-center gap-3 px-4 py-2.5 rounded-full bg-zinc-950/90 backdrop-blur-2xl border border-white/15 shadow-[0_12px_40px_rgba(0,0,0,0.65),0_1px_0_rgba(255,255,255,0.15)_inset] ring-1 ring-black/40"
        >
          {/* Specular gloss top reflection */}
          <div className="absolute inset-x-3 top-0 h-[1px] bg-gradient-to-r from-transparent via-white/35 to-transparent pointer-events-none" />

          <AnimatePresence mode="wait">
            {showPasted ? (
              /* Success / Pasted state */
              <motion.div
                key="pasted"
                initial={{ opacity: 0, scale: 0.8 }}
                animate={{ opacity: 1, scale: 1 }}
                exit={{ opacity: 0, scale: 0.8 }}
                className="flex items-center gap-2 px-1 py-0.5"
              >
                <div className="flex items-center justify-center w-5 h-5 rounded-full bg-emerald-500/20 text-emerald-400 border border-emerald-500/30 shadow-[0_0_12px_rgba(16,185,129,0.3)]">
                  <Check className="w-3.5 h-3.5 stroke-[2.5]" />
                </div>
                <span className="text-[13px] font-semibold text-emerald-300 tracking-tight">
                  Pasted
                </span>
              </motion.div>
            ) : isRecording ? (
              /* Juicy Recording / Listening State */
              <motion.div
                key="recording"
                initial={{ opacity: 0, scale: 0.92 }}
                animate={{ opacity: 1, scale: 1 }}
                exit={{ opacity: 0, scale: 0.92 }}
                transition={{ type: 'spring', stiffness: 450, damping: 30 }}
                className="flex items-center gap-3"
              >
                {/* Radiant Pulsing Recording Beacon */}
                <div className="relative flex items-center justify-center w-4 h-4 shrink-0">
                  <motion.span
                    animate={{
                      scale: [1, 1.8 + audioLevel * 0.8, 1],
                      opacity: [0.75, 0.15, 0.75],
                    }}
                    transition={{
                      repeat: Infinity,
                      duration: 1.6,
                      ease: 'easeInOut',
                    }}
                    className="absolute w-full h-full rounded-full bg-rose-500 blur-[2px]"
                  />
                  <motion.span
                    animate={{ scale: 1 + audioLevel * 0.4 }}
                    className="relative w-2.5 h-2.5 rounded-full bg-gradient-to-tr from-rose-500 to-red-400 shadow-[0_0_8px_rgba(244,63,94,0.8)]"
                  />
                </div>

                {/* State Label & Elapsed Time Badge */}
                <div className="flex items-center gap-2">
                  <span className="text-[13px] font-semibold text-white/95 tracking-tight flex items-center gap-1.5">
                    <span className="bg-gradient-to-r from-white via-zinc-100 to-zinc-300 bg-clip-text text-transparent">
                      Listening
                    </span>
                  </span>
                  <span className="px-1.5 py-0.5 rounded-md bg-white/10 text-[11px] font-mono font-medium text-zinc-300 tabular-nums border border-white/5">
                    {formatTime(elapsedSec)}
                  </span>
                </div>

                {/* Dynamic Multi-Bar Equalizer Waveform */}
                <div className="flex items-center gap-1 h-5 px-1.5 py-0.5 rounded-full bg-white/[0.04] border border-white/5">
                  {[0, 1, 2, 3, 4, 5].map((i) => {
                    const offset = i * 0.5;
                    const minHeight = 4;
                    const maxHeight = 18;
                    const dynamicBoost = audioLevel * (maxHeight - minHeight);
                    const wave = Math.sin(Date.now() * 0.008 + offset) * 0.5 + 0.5;
                    const height = Math.min(
                      maxHeight,
                      Math.max(minHeight, minHeight + dynamicBoost * (0.4 + wave * 0.6) + (i % 2) * 2)
                    );

                    return (
                      <motion.span
                        key={i}
                        animate={{ height: `${height}px` }}
                        transition={{ type: 'spring', stiffness: 500, damping: 20 }}
                        className={`w-1 rounded-full ${
                          i === 2 || i === 3
                            ? 'bg-gradient-to-t from-violet-500 via-fuchsia-400 to-rose-400 shadow-[0_0_6px_rgba(217,70,239,0.5)]'
                            : i === 1 || i === 4
                            ? 'bg-gradient-to-t from-violet-400 to-cyan-300 shadow-[0_0_5px_rgba(56,189,248,0.4)]'
                            : 'bg-white/70'
                        }`}
                      />
                    );
                  })}
                </div>
              </motion.div>
            ) : (
              /* Transcribing / Processing State */
              <motion.div
                key="transcribing"
                initial={{ opacity: 0, scale: 0.92 }}
                animate={{ opacity: 1, scale: 1 }}
                exit={{ opacity: 0, scale: 0.92 }}
                transition={{ type: 'spring', stiffness: 450, damping: 30 }}
                className="flex items-center gap-2.5 px-0.5"
              >
                {/* Glowing Aurora Loader */}
                <div className="relative flex items-center justify-center w-4 h-4 shrink-0">
                  <motion.div
                    animate={{ rotate: 360 }}
                    transition={{ repeat: Infinity, duration: 1.2, ease: 'linear' }}
                    className="w-4 h-4 rounded-full border-2 border-transparent border-t-violet-400 border-r-fuchsia-400 border-b-cyan-400"
                  />
                  <Sparkles className="w-2.5 h-2.5 text-violet-300 absolute" />
                </div>

                <div className="flex items-center gap-1.5">
                  <span className="text-[13px] font-semibold tracking-tight bg-gradient-to-r from-violet-200 via-fuchsia-200 to-cyan-200 bg-clip-text text-transparent animate-pulse">
                    Transcribing…
                  </span>
                </div>
              </motion.div>
            )}
          </AnimatePresence>
        </motion.div>
      </div>
    </div>
  );
}
