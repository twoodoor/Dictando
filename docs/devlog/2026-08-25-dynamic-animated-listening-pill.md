# Dynamic Animated Listening Pill & Mouse-Aware Screen Targeting

- **Date:** 2026-08-25
- **Phase:** Overlay HUD / Polish
- **Status:** done

## What changed
- src-tauri/src/lib.rs:
  - Added native mouse cursor detection (get_cursor_pos()) via Win32 GetCursorPos.
  - Added multi-monitor target selection (get_cursor_monitor()) to identify which monitor bounds contain the mouse cursor at the moment dictation begins.
  - Positioned the overlay pill centered horizontally and floating comfortably above the bottom dock/taskbar on that exact monitor.
  - Enabled click-through pass-through (set_ignore_cursor_events(true)).
- src-tauri/src/audio.rs:
  - Added real-time RMS volume computation inside the CPAL capture thread.
  - Emitted throttled udio-level events (~33 Hz) to the Tauri frontend.
- src-tauri/tauri.conf.json:
  - Expanded overlay window size to 380x84 to accommodate radiant ambient glow and spring pop-in physics without edge clipping.
- src/lib/bridge.ts:
  - Exported onAudioLevel event subscription.
- src/components/Overlay.tsx:
  - Redesigned into a dynamic, glassmorphic capsule pill with multi-color ambient glow aura, live pulsing recording beacon, elapsed recording duration badge, dynamic multi-bar equalizer waveform reacting in real-time to speech volume and harmonic oscillations, and smooth spring layout transitions for listening, transcribing, and pasted states.

## Why
- Provide instant, juicy, and responsive visual feedback during push-to-talk dictation.
- Ensure the listening HUD always appears on the active monitor where the user is working.
