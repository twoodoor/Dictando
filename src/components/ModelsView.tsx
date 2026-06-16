import React, { useEffect, useState } from 'react';
import { Download, Trash2, Loader2, Globe, Cpu } from 'lucide-react';
import { toast } from 'sonner';
import { motion } from 'motion/react';
import { backend, events, isNative, type ModelInfo, type DownloadProgress } from '../lib/bridge';

function formatSize(bytes: number): string {
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
  return `${Math.round(bytes / 1024 ** 2)} MB`;
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

const FILTERS = ['All Languages', 'Multi-language', 'English Only'] as const;

export function ModelsView() {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [activeId, setActiveId] = useState<string>('');
  const [progress, setProgress] = useState<Record<string, DownloadProgress>>({});
  const [filter, setFilter] = useState<(typeof FILTERS)[number]>('All Languages');
  const [busy, setBusy] = useState<string | null>(null);

  const refresh = () => { backend.listModels().then(setModels).catch(() => {}); };

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

  const visible = models.filter((m) => filter === 'All Languages' || m.languages === filter);
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
    return (
      <div className={`bg-surface border rounded-2xl p-4 transition-colors ${isActive ? 'border-accent' : 'border-line'}`}>
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <h3 className="text-sm font-semibold text-fg truncate">{m.name}</h3>
              {isActive && (
                <span className="text-[10px] font-medium px-1.5 py-0.5 rounded bg-accent-soft text-accent border border-accent">Active</span>
              )}
            </div>
            <p className="text-xs text-muted mt-0.5">{m.description}</p>
          </div>
          <div className="w-24 shrink-0 space-y-1">
            <Meter label="accuracy" value={m.accuracy} />
            <Meter label="speed" value={m.speed} />
          </div>
        </div>

        <div className="flex items-center justify-between mt-3 pt-3 border-t border-line">
          <span className="flex items-center gap-1.5 text-[11px] text-muted"><Globe size={12} /> {m.languages}</span>
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
              <Download size={12} /> {formatSize(m.sizeBytes)}
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
            onChange={(e) => setFilter(e.target.value as (typeof FILTERS)[number])}
            className="bg-surface border border-line text-fg text-xs rounded-lg px-2 py-1.5 focus:outline-none focus:border-accent"
          >
            {FILTERS.map((f) => <option key={f} value={f}>{f}</option>)}
          </select>
        </div>
        <p className="text-muted text-sm mb-6">Download fast, accurate models that run entirely on your device.</p>

        {installed.length > 0 && (
          <>
            <p className="text-[11px] uppercase tracking-wide text-faint mb-2">Downloaded</p>
            <div className="space-y-2 mb-6">{installed.map((m) => <Card key={m.id} m={m} />)}</div>
          </>
        )}
        <p className="text-[11px] uppercase tracking-wide text-faint mb-2">Available to download</p>
        <div className="space-y-2 pb-4">
          {available.length ? available.map((m) => <Card key={m.id} m={m} />)
            : <p className="text-center text-muted text-sm py-8">No models match this filter.</p>}
        </div>
      </div>
    </motion.div>
  );
}
