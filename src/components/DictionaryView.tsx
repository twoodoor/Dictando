import React, { useEffect, useState } from 'react';
import { Plus, X, BookA } from 'lucide-react';
import { toast } from 'sonner';
import { motion } from 'motion/react';
import { isNative, backend } from '../lib/bridge';

/**
 * Dictionary — custom words/names Dictando should get right (people, brands,
 * jargon). Persisted to `customWords` in native settings. (Biasing the model
 * with these is a later enhancement; today they're stored and shown here.)
 */
export function DictionaryView() {
  const [words, setWords] = useState<string[]>([]);
  const [draft, setDraft] = useState('');

  useEffect(() => {
    if (!isNative) return;
    backend.getSettings().then((s) => setWords(s.customWords || [])).catch(() => {});
  }, []);

  const persist = (next: string[]) => {
    setWords(next);
    backend.updateSettings({ customWords: next } as any).catch(() => toast.error('Failed to save'));
  };

  const add = () => {
    const w = draft.trim();
    if (!w) return;
    if (words.some((x) => x.toLowerCase() === w.toLowerCase())) {
      toast.message('Already in your dictionary');
      setDraft('');
      return;
    }
    persist([w, ...words]);
    setDraft('');
  };

  const remove = (w: string) => persist(words.filter((x) => x !== w));

  return (
    <motion.div
      initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -8 }}
      className="absolute inset-0 overflow-y-auto"
    >
      <div className="max-w-3xl mx-auto px-8 py-8">
        <h1 className="font-display text-3xl text-fg">Dictionary</h1>
        <p className="text-muted text-sm mt-1">
          Teach Mumblr the words and names you use — people, brands, technical terms.
        </p>

        {!isNative ? (
          <div className="mt-8 text-center text-muted text-sm">Available in the desktop app.</div>
        ) : (
          <>
            <div className="mt-6 flex gap-2">
              <input
                value={draft}
                onChange={(e) => setDraft(e.target.value)}
                onKeyDown={(e) => { if (e.key === 'Enter') add(); }}
                placeholder="Add a word or name…"
                className="flex-1 bg-surface border border-line rounded-xl px-4 py-2.5 text-sm text-fg placeholder:text-faint focus:outline-none focus:border-accent"
              />
              <button
                onClick={add}
                className="px-4 rounded-xl bg-accent text-accent-fg text-sm font-medium flex items-center gap-1.5 hover:bg-accent-strong transition-colors"
              >
                <Plus size={16} /> Add
              </button>
            </div>

            {words.length === 0 ? (
              <div className="mt-12 flex flex-col items-center text-center text-muted">
                <span className="w-12 h-12 rounded-2xl bg-surface-2 flex items-center justify-center mb-3">
                  <BookA size={22} className="text-faint" />
                </span>
                <p className="text-sm">No custom words yet.</p>
                <p className="text-xs text-faint mt-1">Add names or terms you dictate often.</p>
              </div>
            ) : (
              <div className="mt-6 flex flex-wrap gap-2">
                {words.map((w) => (
                  <span
                    key={w}
                    className="group flex items-center gap-1.5 bg-surface border border-line rounded-full pl-3.5 pr-2 py-1.5 text-sm text-fg"
                  >
                    {w}
                    <button
                      onClick={() => remove(w)}
                      className="w-5 h-5 rounded-full flex items-center justify-center text-faint hover:bg-surface-2 hover:text-danger transition-colors"
                      aria-label={`Remove ${w}`}
                    >
                      <X size={13} />
                    </button>
                  </span>
                ))}
              </div>
            )}
          </>
        )}
      </div>
    </motion.div>
  );
}
