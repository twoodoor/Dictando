/**
 * bridge.ts — IPC abstraction between the React UI and the Dictando backend.
 *
 * Dictando runs in two environments:
 *   1. Native (Tauri): a Rust backend handles global-shortcut recording, local
 *      transcription, and paste-injection, exposing Tauri `invoke` commands and
 *      emitting events.
 *   2. Web / PWA: no native backend; transcription happens in-browser (Gemini)
 *      and features that require OS integration are unavailable.
 *
 * Components should import from here instead of calling `invoke`/`listen`
 * directly, so the same code degrades gracefully on web. When not running under
 * Tauri, command calls reject with `NOT_NATIVE` and event subscriptions are
 * no-ops returning a cleanup function.
 */

// ---------------------------------------------------------------------------
// Environment detection
// ---------------------------------------------------------------------------

/** True when running inside the native Tauri shell. */
export const isNative: boolean =
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

const NOT_NATIVE = new Error('NOT_NATIVE: this feature requires the desktop app');

// ---------------------------------------------------------------------------
// Shared types (mirror the Rust serde structs in src-tauri/src/settings.rs)
// ---------------------------------------------------------------------------

export type PasteMethod = 'direct' | 'clipboard';
export type ClipboardHandling = 'preserve' | 'overwrite';
export type OverlayPosition = 'top' | 'bottom' | 'none';

export type AiStylePreset = 'clean' | 'polished' | 'concise' | 'casual';

export interface AppSettings {
  /** Global shortcut as KeyboardEvent.code values, e.g. ["ControlLeft","Space"]. */
  shortcut: string[];
  pushToTalk: boolean;
  language: string; // "Auto-detect" | "English" | ...
  activeModelId: string; // e.g. "parakeet-tdt-0.6b-v3"
  microphoneId: string; // "default" | device id
  muteWhileRecording: boolean;
  audioFeedback: boolean;
  pasteMethod: PasteMethod;
  clipboardHandling: ClipboardHandling;
  appendTrailingSpace: boolean;
  autoSubmit: boolean;
  customWords: string[];
  overlayPosition: OverlayPosition;
  launchOnStartup: boolean;
  startHidden: boolean;
  showTrayIcon: boolean;
  unloadModelMinutes: number; // 0 = never unload
  historyLimit: number;
  theme: 'light' | 'dark' | 'system';
  // AI enhancement layer (Phase 4) — off by default keeps transcription offline.
  aiEnhanceEnabled: boolean;
  geminiApiKey: string;
  aiFixPunctuation: boolean;
  aiRemoveFillers: boolean;
  aiRemoveRepetitions: boolean;
  aiStylePreset: AiStylePreset;
  aiCustomInstructions: string;
}

export interface TranscriptionResult {
  id: string;
  text: string;
  durationMs: number;
  engine: string; // model id used
  timestamp: number; // epoch ms
}

export type ModelFormat = 'onnx' | 'ggml';

export interface ModelInfo {
  id: string;
  name: string;
  description: string;
  languages: string; // "Multi-language" | "English Only" | ...
  /** Approximate download size in bytes. */
  sizeBytes: number;
  accuracy: number; // 0..1 for the UI bar
  speed: number; // 0..1 for the UI bar
  format: ModelFormat;
  supportsTranslation: boolean;
  installed: boolean;
}

export interface DownloadProgress {
  modelId: string;
  downloadedBytes: number;
  totalBytes: number;
  /** "downloading" | "extracting" | "verifying" | "done" | "error" */
  phase: string;
  error?: string;
}

export type RecordingState = 'idle' | 'recording' | 'transcribing';

export interface BackendStatus {
  recordingState: RecordingState;
  modelLoaded: boolean;
  activeModelId: string | null;
}

// ---------------------------------------------------------------------------
// Low-level invoke / listen wrappers (dynamically import the Tauri API so the
// web build does not hard-depend on it at module load).
// ---------------------------------------------------------------------------

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isNative) throw NOT_NATIVE;
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(cmd, args);
}

/** Subscribe to a backend event. Returns an unsubscribe function. */
export function listen<T>(event: string, handler: (payload: T) => void): () => void {
  if (!isNative) return () => {};
  let unlisten: (() => void) | undefined;
  let cancelled = false;
  import('@tauri-apps/api/event').then(({ listen }) => {
    if (cancelled) return;
    listen<T>(event, (e) => handler(e.payload)).then((u) => {
      if (cancelled) u();
      else unlisten = u;
    });
  });
  return () => {
    cancelled = true;
    unlisten?.();
  };
}

// ---------------------------------------------------------------------------
// Public command API (each maps to a #[tauri::command] in the Rust backend)
// ---------------------------------------------------------------------------

export const backend = {
  // Settings
  getSettings: () => invoke<AppSettings>('get_settings'),
  updateSettings: (patch: Partial<AppSettings>) =>
    invoke<AppSettings>('update_settings', { patch }),

  // Status & recording (in-app button; the global shortcut is handled in Rust)
  getStatus: () => invoke<BackendStatus>('get_status'),
  startRecording: () => invoke<void>('start_recording'),
  stopRecording: () => invoke<void>('stop_recording'),

  // Models (Phase 2)
  listModels: () => invoke<ModelInfo[]>('list_models'),
  downloadModel: (modelId: string) => invoke<void>('download_model', { modelId }),
  deleteModel: (modelId: string) => invoke<void>('delete_model', { modelId }),
  setActiveModel: (modelId: string) => invoke<void>('set_active_model', { modelId }),

  // History (Phase 3) — local SQLite
  listHistory: (limit?: number) => invoke<TranscriptionResult[]>('list_history', { limit }),
  deleteHistory: (id: string) => invoke<void>('delete_history', { id }),
  clearHistory: () => invoke<void>('clear_history'),
  openRecordingsFolder: () => invoke<void>('open_recordings_folder'),

  // Audio devices
  listMicrophones: () => invoke<{ id: string; label: string }[]>('list_microphones'),

  // Platform / macOS permissions (first-run UX)
  getPlatform: () => invoke<string>('get_platform'),
  accessibilityStatus: () => invoke<boolean>('accessibility_status'),
  requestAccessibility: () => invoke<boolean>('request_accessibility'),
  openAccessibilitySettings: () => invoke<void>('open_accessibility_settings'),
};

// ---------------------------------------------------------------------------
// Event subscriptions (each maps to an app.emit in the Rust backend)
// ---------------------------------------------------------------------------

export const events = {
  onRecordingState: (h: (state: RecordingState) => void) =>
    listen<RecordingState>('recording-state', h),
  onTranscription: (h: (r: TranscriptionResult) => void) =>
    listen<TranscriptionResult>('transcription', h),
  onModelStatus: (h: (s: BackendStatus) => void) =>
    listen<BackendStatus>('model-status', h),
  onDownloadProgress: (h: (p: DownloadProgress) => void) =>
    listen<DownloadProgress>('download-progress', h),
};

export interface AppUpdateInfo {
  version: string;
  body?: string;
  date?: string;
  downloadAndInstall: () => Promise<void>;
}

export async function checkForAppUpdates(): Promise<AppUpdateInfo | null> {
  if (!isNative) return null;
  try {
    const { check } = await import('@tauri-apps/plugin-updater');
    const update = await check();
    if (!update) return null;
    return {
      version: update.version,
      body: update.body || undefined,
      date: update.date || undefined,
      downloadAndInstall: async () => {
        await update.downloadAndInstall();
        const { relaunch } = await import('@tauri-apps/plugin-process');
        await relaunch();
      },
    };
  } catch (err) {
    console.warn('Update check failed:', err);
    throw err;
  }
}
