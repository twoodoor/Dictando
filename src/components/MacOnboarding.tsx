import React, { useEffect, useState } from 'react';
import { Mic, Accessibility, Check, ExternalLink } from 'lucide-react';
import { motion } from 'motion/react';
import { backend } from '../lib/bridge';

/**
 * macOS first-run permissions onboarding. Shown on macOS until Accessibility is
 * granted (required for the global shortcut + keystroke injection). Also nudges
 * the microphone grant. No-op on other platforms (App gates rendering).
 */
export function MacOnboarding({ onDone }: { onDone: () => void }) {
  const [micRequested, setMicRequested] = useState(false);
  const [a11y, setA11y] = useState(false);

  const refreshA11y = () => backend.accessibilityStatus().then(setA11y).catch(() => {});

  useEffect(() => {
    refreshA11y();
    // Re-check when returning from System Settings.
    const onFocus = () => refreshA11y();
    window.addEventListener('focus', onFocus);
    const iv = setInterval(refreshA11y, 2000);
    return () => { window.removeEventListener('focus', onFocus); clearInterval(iv); };
  }, []);

  const enableMic = async () => {
    try {
      const s = await navigator.mediaDevices.getUserMedia({ audio: true });
      s.getTracks().forEach((t) => t.stop());
    } catch { /* user can still grant later in System Settings */ }
    setMicRequested(true);
  };

  const grantA11y = async () => {
    await backend.requestAccessibility().catch(() => {});
    await backend.openAccessibilitySettings().catch(() => {});
  };

  const Step = ({
    icon, title, desc, done, children,
  }: { icon: React.ReactNode; title: string; desc: string; done: boolean; children: React.ReactNode }) => (
    <div className="flex gap-3 p-4 rounded-2xl bg-surface border border-line">
      <div className={`w-9 h-9 shrink-0 rounded-xl flex items-center justify-center ${done ? 'bg-emerald-500/15 text-emerald-500' : 'bg-accent-soft text-accent'}`}>
        {done ? <Check size={18} /> : icon}
      </div>
      <div className="flex-1 min-w-0">
        <div className="text-sm font-medium text-fg">{title}</div>
        <div className="text-xs text-muted mt-0.5 mb-2">{desc}</div>
        {!done && children}
        {done && <div className="text-xs font-medium text-emerald-500">Granted</div>}
      </div>
    </div>
  );

  return (
    <div className="absolute inset-0 z-50 bg-app/80 backdrop-blur-sm flex items-center justify-center p-6">
      <motion.div
        initial={{ opacity: 0, y: 12 }} animate={{ opacity: 1, y: 0 }}
        className="w-full max-w-md bg-app border border-line rounded-3xl shadow-2xl p-6"
      >
        <h2 className="font-display text-2xl text-fg">Welcome to Dictando</h2>
        <p className="text-sm text-muted mt-1 mb-5">Two quick macOS permissions and you're set.</p>

        <div className="space-y-3">
          <Step
            icon={<Mic size={18} />}
            title="Microphone"
            desc="So Dictando can hear and transcribe your speech (on-device)."
            done={micRequested}
          >
            <button onClick={enableMic} className="px-3 py-1.5 rounded-lg bg-accent text-accent-fg text-xs font-medium hover:bg-accent-strong transition-colors">
              Enable microphone
            </button>
          </Step>

          <Step
            icon={<Accessibility size={18} />}
            title="Accessibility"
            desc="Lets the global shortcut work and types text into other apps."
            done={a11y}
          >
            <button onClick={grantA11y} className="px-3 py-1.5 rounded-lg bg-accent text-accent-fg text-xs font-medium hover:bg-accent-strong transition-colors inline-flex items-center gap-1.5">
              Open Accessibility settings <ExternalLink size={12} />
            </button>
            <p className="text-[11px] text-faint mt-2">Toggle Dictando on in the list, then return here.</p>
          </Step>
        </div>

        <button
          onClick={onDone}
          className="w-full mt-5 py-2.5 rounded-xl bg-accent text-accent-fg text-sm font-semibold hover:bg-accent-strong transition-colors disabled:opacity-50"
          disabled={!a11y}
        >
          {a11y ? "Start dictating" : "Waiting for Accessibility…"}
        </button>
        <button onClick={onDone} className="w-full mt-2 text-xs text-muted hover:text-fg transition-colors">
          Skip for now
        </button>
      </motion.div>
    </div>
  );
}
