import React, { useEffect, useMemo, useState } from 'react';
import {
  Download, Trash2, Loader2, Globe, Cpu, ChevronDown, Timer, Target, HardDrive,
  MemoryStick, AlertTriangle, Check, X, Gauge,
} from 'lucide-react';
import { toast } from 'sonner';
import { motion } from 'motion/react';
import {
  backend, events, isNative,
  type ModelInfo, type DownloadProgress, type ModelStats,
} from '../lib/bridge';

function formatSize(bytes: number): string {
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
  return `${Math.round(bytes / 1024 ** 2)} MB`;
}

function formatMs(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)} ms`;
  return `${(ms / 1000).toFixed(ms < 10_000 ? 1 : 0)} s`;
}

function formatRam(mb: number): string {
  return mb >= 1000 ? `${(mb / 1024).toFixed(1)} GB` : `${mb} MB`;
}

function Meter({ label, value }: { label: string; value: number }) {
  return (
    <div className="flex items-center gap-2">
      <span className="text-[10px] text-faint w-12 text-right">{label}</span>
      <div className="flex-1 h-1.5 bg-surface-2 rounded-full overflow-hidden">
        <div className="h-full bg-accent rounded-full" style={{ width: `${Math.round(value * 100)}%` }} />
      </div>
    </div>
  );
}

/** One labelled number in the stats grid. */
function Stat({ icon, label, value, hint }: { icon: React.ReactNode; label: string; value: string; hint?: string }) {
  return (
    <div className="bg-surface-2 rounded-xl px-3 py-2 min-w-0">
      <div className="flex items-center gap-1 text-[10px] uppercase tracking-wide text-faint">{icon}{label}</div>
      <div className="text-sm font-semibold text-fg mt-0.5 truncate">{value}</div>
      {hint && <div className="text-[10px] text-muted truncate" title={hint}>{hint}</div>}
    </div>
  );
}

function Flag({ on, label }: { on: boolean; label: string }) {
  return (
    <span className={`inline-flex items-center gap-1 text-[11px] ${on ? 'text-fg' : 'text-faint'}`}>
      {on ? <Check size={12} className="text-ok" /> : <X size={12} />} {label}
    </span>
  );
}

const ALL = 'All languages';
const LANG_PREVIEW = 10;

export function ModelsView() {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [stats, setStats] = useState<Record<string, ModelStats>>({});
  const [activeId, setActiveId] = useState<string>('');
  const [progress, setProgress] = useState<Record<string, DownloadProgress>>({});
  const [filter, setFilter] = useState<string>(ALL);
  const [busy, setBusy] = useState<string | null>(null);
  const [open, setOpen] = useState<Record<string, boolean>>({});
  const [allLangs, setAllLangs] = useState<Record<string, boolean>>({});

  const refresh = () => {
    backend.listModels().then(setModels).catch(() => {});
    backend.modelStats()
      .then((list) => setStats(Object.fromEntries(list.map((s) => [s.modelId, s]))))
      .catch(() => {});
  };

  useEffect(() => {
    if (!isNative) return;
    refresh();
    backend.getSettings().then((s) => setActiveId(s.activeModelId)).catch(() => {});
    const off = events.onDownloadProgress((p) => {
      setProgress((prev) => ({ ...prev, [p.modelId]: p }));
      if (p.phase === 'done') {
        toast.success('Model installed');
        setProgress((prev) => { const n = { ...prev }; delete n[p.modelId]; return n; });
        refresh();
      } else if (p.phase === 'error') {
        toast.error(p.error || 'Download failed');
        setProgress((prev) => { const n = { ...prev }; delete n[p.modelId]; return n; });
      }
    });
    return off;
  }, []);

  const download = (id: string) => {
    setProgress((prev) => ({ ...prev, [id]: { modelId: id, downloadedBytes: 0, totalBytes: 0, phase: 'downloading' } }));
    backend.downloadModel(id).catch((e) => toast.error(String(e)));
  };
  const remove = async (id: string) => {
    setBusy(id);
    try { await backend.deleteModel(id); toast.success('Deleted'); refresh(); }
    catch { toast.error('Delete failed'); }
    setBusy(null);
  };
  const activate = async (id: string) => {
    setBusy(id);
    try { await backend.setActiveModel(id); setActiveId(id); toast.success('Active model set'); }
    catch { toast.error('Failed'); }
    setBusy(null);
  };

  // Language filter: every language any model supports, English first.
  const languages = useMemo(() => {
    const set = new Set<string>();
    models.forEach((m) => m.languageList?.forEach((l) => set.add(l)));
    const sorted = [...set].sort((a, b) => a.localeCompare(b));
    return [ALL, ...(set.has('English') ? ['English'] : []), ...sorted.filter((l) => l !== 'English')];
  }, [models]);

  const visible = models.filter((m) => filter === ALL || m.languageList?.includes(filter));
  const installed = visible.filter((m) => m.installed);
  const available = visible.filter((m) => !m.installed);

  if (!isNative) {
    return (
      <div className="absolute inset-0 flex items-center justify-center p-8 text-center">
        <p className="text-muted text-sm">Local models are available in the desktop app.</p>
      </div>
    );
  }

  const Card = ({ m }: { m: ModelInfo }) => {
    const dl = progress[m.id];
    const pct = dl && dl.totalBytes > 0 ? Math.round((dl.downloadedBytes / dl.totalBytes) * 100) : 0;
    const isActive = m.id === activeId;
    const expanded = !!open[m.id];
    const mine = stats[m.id];
    const langs = m.languageList ?? [];
    const showAll = !!allLangs[m.id];
    const langSummary = langs.length === 1 ? `${langs[0]} only` : `${langs.length} languages`;

    return (
      <div className={`bg-surface border rounded-2xl p-4 transition-colors ${isActive ? 'border-accent' : 'border-line'}`}>
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0">
            <div className="flex items-center gap-2 flex-wrap">
              <h3 className="text-sm font-semibold text-fg truncate">{m.name}</h3>
              {isActive && (
                <span className="text-[10px] font-medium px-1.5 py-0.5 rounded bg-accent-soft text-accent border border-accent">Active</span>
              )}
              {!m.dictationReady && (
                <span className="inline-flex items-center gap-1 text-[10px] font-medium px-1.5 py-0.5 rounded border border-line text-danger">
                  <AlertTriangle size={10} /> Too slow for dictation
                </span>
              )}
            </div>
            <p className="text-xs text-muted mt-0.5">{m.description}</p>
          </div>
          <div className="w-24 shrink-0 space-y-1">
            <Meter label="accuracy" value={m.accuracy} />
            <Meter label="speed" value={m.speed} />
          </div>
        </div>

        {/* At-a-glance numbers */}
        <div className="grid grid-cols-2 sm:grid-cols-4 gap-2 mt-3">
          <Stat
            icon={<Timer size={10} />}
            label="Latency"
            value={`${m.latencyMeasured ? '' : '~'}${formatMs(m.latencyMs)}`}
            hint={m.latencyMeasured ? 'measured, 4 s clip, Ryzen 9 5950X' : 'estimate, 4 s clip, desktop CPU'}
          />
          <Stat
            icon={<Gauge size={10} />}
            label="On this PC"
            value={mine ? formatMs(mine.avgLatencyMs) : '—'}
            hint={mine ? `avg of ${mine.count} dictation${mine.count === 1 ? '' : 's'}` : 'no dictations yet'}
          />
          <Stat
            icon={<Target size={10} />}
            label="Error rate"
            value={m.werPct != null ? `${m.werPct.toFixed(1)}% WER` : '—'}
            hint={m.werPct != null ? 'English benchmark, lower is better' : 'not benchmarked'}
          />
          <Stat
            icon={<HardDrive size={10} />}
            label="Size"
            value={formatSize(m.sizeBytes)}
            hint={`~${formatRam(m.ramMb)} RAM while loaded`}
          />
        </div>

        {/* Expandable details */}
        <button
          onClick={() => setOpen((o) => ({ ...o, [m.id]: !expanded }))}
          className="mt-3 flex items-center gap-1 text-[11px] text-muted hover:text-fg transition-colors"
        >
          <ChevronDown size={12} className={`transition-transform ${expanded ? 'rotate-180' : ''}`} />
          {expanded ? 'Hide details' : 'Details, languages & limitations'}
        </button>

        {expanded && (
          <div className="mt-3 space-y-3 text-xs">
            <div>
              <p className="text-[10px] uppercase tracking-wide text-faint mb-1">Best for</p>
              <p className="text-fg">{m.bestFor}</p>
            </div>

            {m.notes?.length > 0 && (
              <div>
                <p className="text-[10px] uppercase tracking-wide text-faint mb-1">Good to know</p>
                <ul className="space-y-1">
                  {m.notes.map((n, i) => (
                    <li key={i} className="flex gap-2 text-muted"><span className="text-faint">•</span><span>{n}</span></li>
                  ))}
                </ul>
              </div>
            )}

            <div className="flex flex-wrap gap-x-4 gap-y-1">
              <Flag on={m.punctuation} label="Punctuation & capitals" />
              <Flag on={m.usesDictionary} label="Uses Dictionary hints" />
              <Flag on={m.supportsTranslation} label="Can translate to English" />
              <Flag on label="Runs offline" />
            </div>

            <div>
              <p className="text-[10px] uppercase tracking-wide text-faint mb-1">Languages ({langs.length})</p>
              <div className="flex flex-wrap gap-1">
                {(showAll ? langs : langs.slice(0, LANG_PREVIEW)).map((l) => (
                  <span
                    key={l}
                    className={`px-1.5 py-0.5 rounded-md border text-[11px] ${l === filter ? 'border-accent text-accent bg-accent-soft' : 'border-line text-muted'}`}
                  >{l}</span>
                ))}
                {langs.length > LANG_PREVIEW && (
                  <button
                    onClick={() => setAllLangs((a) => ({ ...a, [m.id]: !showAll }))}
                    className="px-1.5 py-0.5 rounded-md text-[11px] text-accent hover:underline"
                  >{showAll ? 'show less' : `+${langs.length - LANG_PREVIEW} more`}</button>
                )}
              </div>
            </div>

            <div className="flex flex-wrap gap-x-4 gap-y-1 text-[11px] text-muted">
              <span className="flex items-center gap-1"><Cpu size={11} /> {m.params} parameters</span>
              <span className="flex items-center gap-1"><MemoryStick size={11} /> ~{formatRam(m.ramMb)} RAM</span>
              <span>{m.architecture}</span>
              {mine && mine.msPerAudioSecond > 0 && (
                <span>{formatMs(mine.msPerAudioSecond)} per second of speech on this PC</span>
              )}
            </div>
          </div>
        )}

        <div className="flex items-center justify-between mt-3 pt-3 border-t border-line">
          <span className="flex items-center gap-1.5 text-[11px] text-muted" title={langs.join(', ')}>
            <Globe size={12} /> {langSummary}
          </span>
          {dl ? (
            <div className="flex items-center gap-2 w-40">
              <div className="flex-1 h-1.5 bg-surface-2 rounded-full overflow-hidden">
                <div className="h-full bg-accent transition-all" style={{ width: `${pct}%` }} />
              </div>
              <span className="text-[10px] text-muted w-16 text-right capitalize">{dl.phase === 'downloading' ? `${pct}%` : dl.phase}</span>
            </div>
          ) : m.installed ? (
            <div className="flex items-center gap-3">
              {!isActive && (
                <button onClick={() => activate(m.id)} disabled={busy === m.id} className="flex items-center gap-1 text-[11px] text-muted hover:text-accent transition-colors">
                  <Cpu size={12} /> Use
                </button>
              )}
              <button onClick={() => remove(m.id)} disabled={busy === m.id || isActive} className="flex items-center gap-1 text-[11px] text-muted hover:text-danger transition-colors disabled:opacity-40">
                {busy === m.id ? <Loader2 size={12} className="animate-spin" /> : <Trash2 size={12} />} Delete
              </button>
            </div>
          ) : (
            <button onClick={() => download(m.id)} className="flex items-center gap-1.5 text-[11px] text-muted hover:text-accent transition-colors">
              <Download size={12} /> Download · {formatSize(m.sizeBytes)}
            </button>
          )}
        </div>
      </div>
    );
  };

  return (
    <motion.div
      initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -8 }}
      className="absolute inset-0 overflow-y-auto"
    >
      <div className="max-w-3xl mx-auto px-8 py-8">
        <div className="flex items-end justify-between mb-1">
          <h1 className="font-display text-3xl text-fg">Models</h1>
          <select
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            className="bg-surface border border-line text-fg text-xs rounded-lg px-2 py-1.5 focus:outline-none focus:border-accent"
            aria-label="Filter by language"
          >
            {languages.map((f) => <option key={f} value={f}>{f}</option>)}
          </select>
        </div>
        <p className="text-muted text-sm mb-6">
          Download fast, accurate models that run entirely on your device. Latency is how long the model
          takes to turn a typical 4-second sentence into text after you release the shortcut.
        </p>

        {installed.length > 0 && (
          <>
            <p className="text-[11px] uppercase tracking-wide text-faint mb-2">Downloaded</p>
            <div className="space-y-2 mb-6">{installed.map((m) => <Card key={m.id} m={m} />)}</div>
          </>
        )}
        <p className="text-[11px] uppercase tracking-wide text-faint mb-2">Available to download</p>
        <div className="space-y-2 pb-4">
          {available.length ? available.map((m) => <Card key={m.id} m={m} />)
            : <p className="text-center text-muted text-sm py-8">
                {filter === ALL ? 'Every model is downloaded.' : `No other models support ${filter}.`}
              </p>}
        </div>
      </div>
    </motion.div>
  );
}
