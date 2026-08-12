import React, { useEffect, useState } from 'react';
import { events, type RecordingState } from '../lib/bridge';

/**
 * Recording HUD shown in the always-on-top `overlay` window. Displays live
 * streaming partial transcription text as the user speaks.
 */
export function Overlay() {
  const [state, setState] = useState<RecordingState>('recording');
  const [partialText, setPartialText] = useState<string>('');

  useEffect(() => {
    const unlistenState = events.onRecordingState((newState) => {
      setState(newState);
      if (newState === 'idle') {
        setPartialText('');
      }
    });

    const unlistenPartial = events.onPartialTranscription((p) => {
      setPartialText(p.text);
    });

    return () => {
      unlistenState();
      unlistenPartial();
    };
  }, []);

  const recording = state === 'recording';

  return (
    <div className="w-screen h-screen flex items-center justify-center bg-transparent select-none">
      <div className="flex items-center gap-2.5 px-4 py-2 rounded-full bg-zinc-900/90 backdrop-blur border border-white/10 shadow-2xl max-w-[85vw] transition-all duration-200">
        {recording ? (
          <>
            <span className="relative flex h-2.5 w-2.5 shrink-0">
              <span className="absolute inline-flex h-full w-full rounded-full bg-red-500 opacity-70 animate-ping" />
              <span className="relative inline-flex h-2.5 w-2.5 rounded-full bg-red-500" />
            </span>
            <span className="text-[12px] font-medium text-white truncate">
              {partialText ? partialText : 'Listening…'}
            </span>
            <div className="flex items-end gap-0.5 h-3.5 shrink-0">
              {[0, 1, 2, 3].map((i) => (
                <span
                  key={i}
                  className="w-0.5 bg-white/70 rounded-full animate-pulse"
                  style={{ height: `${6 + (i % 2) * 6}px`, animationDelay: `${i * 120}ms` }}
                />
              ))}
            </div>
          </>
        ) : (
          <>
            <span className="h-3 w-3 rounded-full border-2 border-violet-400 border-t-transparent animate-spin shrink-0" />
            <span className="text-[12px] font-medium text-white truncate max-w-[400px]">
              {partialText ? partialText : 'Transcribing…'}
            </span>
          </>
        )}
      </div>
    </div>
  );
}
